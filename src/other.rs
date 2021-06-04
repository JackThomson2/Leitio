use flize::{unprotected, Atomic, Collector, Shared, Shield, ThinShield};
/// Based on https://github.com/xacrimon/flize/blob/3358915c7d13c09a04d34537869c0f380339b298/examples/queue/src/lib.rs#L1
use flize::{CachePadded, NullTag};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::usize;

const BUFFER_SIZE: usize = 1024;
const BUFFER_SIZE_MINUS: usize = BUFFER_SIZE - 1;

pub struct Queue<T: Clone> {
    collector: Collector,
    head: Atomic<Node<T>, NullTag, NullTag, 0, 0>,
    tail: Atomic<Node<T>, NullTag, NullTag, 0, 0>,
}

impl<T: Clone> Queue<T> {
    #[inline]
    fn cas_tail<'s, S>(
        &self,
        current: Shared<Node<T>, NullTag, NullTag, 0, 0>,
        new: Shared<Node<T>, NullTag, NullTag, 0, 0>,
        shield: &S,
    ) -> bool
    where
        S: Shield<'s>,
    {
        self.tail
            .compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst, shield)
            .is_ok()
    }

    #[inline]
    fn cas_head<'s, S>(
        &self,
        current: Shared<Node<T>, NullTag, NullTag, 0, 0>,
        new: Shared<Node<T>, NullTag, NullTag, 0, 0>,
        shield: &S,
    ) -> bool
    where
        S: Shield<'s>,
    {
        self.head
            .compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst, shield)
            .is_ok()
    }

    #[inline]
    pub fn new() -> Self {
        let sentinel = Node::empty();

        Self {
            collector: Collector::new(),
            head: Atomic::new(sentinel),
            tail: Atomic::new(sentinel),
        }
    }

    #[inline]
    pub fn get_shield(&self) -> ThinShield {
        self.collector.thin_shield()
    }

    #[inline]
    pub fn push(&self, value: T) {
        let shield = self.collector.thin_shield();

        let new_item = Box::into_raw(Box::new(value));
        let shared = unsafe { Shared::from_ptr(new_item) };

        loop {
            let ltail = self.tail.load(Ordering::SeqCst, &shield);
            let ltailr = unsafe { ltail.as_ref_unchecked() };
            let idx = ltailr.enqidx.fetch_add(1, Ordering::SeqCst);

            if idx > BUFFER_SIZE_MINUS {
                if ltail != self.tail.load(Ordering::SeqCst, &shield) {
                    continue;
                }

                let lnext = ltailr.next.load(Ordering::SeqCst, &shield);

                if lnext.is_null() {
                    let new_node = Node::new(&shared);

                    if ltailr.cas_next(Shared::null(), new_node, &shield) {
                        self.cas_tail(ltail, new_node, &shield);
                        return;
                    }

                    unsafe {
                        Box::from_raw(new_node.as_ptr());
                    }
                } else {
                    self.cas_tail(ltail, lnext, &shield);
                }
            } else if ltailr.items[idx]
                .compare_exchange(
                    Shared::null(),
                    shared,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                    &shield,
                )
                .is_ok()
            {
                return;
            }
        }
    }

    #[inline]
    pub fn pop<'s, S>(&self, shield: &'s S) -> Option<&'s T>
    where
        S: Shield<'s>,
    {
        loop {
            let lhead = self.head.load(Ordering::SeqCst, shield);
            let lheadr = unsafe { lhead.as_ref_unchecked() };

            if lheadr.deqidx.load(Ordering::SeqCst) >= lheadr.enqidx.load(Ordering::SeqCst)
                && lheadr.next.load(Ordering::SeqCst, shield).is_null()
            {
                break None;
            }

            let idx = lheadr.deqidx.fetch_add(1, Ordering::SeqCst);

            if idx > BUFFER_SIZE_MINUS {
                let lnext = lheadr.next.load(Ordering::SeqCst, shield);

                if lnext.is_null() {
                    break None;
                }

                if self.cas_head(lhead, lnext, shield) {
                    shield.retire(move || unsafe {
                        Box::from_raw(lhead.as_ptr());
                    });
                }

                continue;
            }

            let item = lheadr.items[idx].swap(Shared::null(), Ordering::SeqCst, shield);

            if item.is_null() {
                continue;
            }

            return Some(unsafe { item.as_ref_unchecked() });
        }
    }
}

impl<T: Clone> Drop for Queue<T> {
    fn drop(&mut self) {
        unsafe {
            let shield = flize::unprotected();
            while let Some(_) = self.pop(shield) {}
            let lhead = self.head.load(Ordering::SeqCst, shield);
            Box::from_raw(lhead.as_ptr());
        }
    }
}

impl<T: Clone> Default for Queue<T> {
    fn default() -> Self {
        Self::new()
    }
}

struct Node<T: Clone> {
    enqidx: CachePadded<AtomicUsize>,
    deqidx: CachePadded<AtomicUsize>,
    items: Vec<Atomic<T, NullTag, NullTag, 0, 0>>,
    next: Atomic<Self, NullTag, NullTag, 0, 0>,
}

impl<T: Clone> Node<T> {
    #[inline]
    fn empty<'a>() -> Shared<'a, Node<T>, NullTag, NullTag, 0, 0> {
        const START_ENQ: usize = 0;
        let items = Atomic::null_vec(BUFFER_SIZE);

        let raw = Box::into_raw(Box::new(Self {
            enqidx: CachePadded::new(AtomicUsize::new(START_ENQ)),
            deqidx: CachePadded::new(AtomicUsize::new(START_ENQ)),
            items,
            next: Atomic::null(),
        }));

        unsafe { Shared::from_ptr(raw) }
    }

    #[inline]
    fn new<'a>(
        first: &Shared<T, NullTag, NullTag, 0, 0>,
    ) -> Shared<'a, Node<T>, NullTag, NullTag, 0, 0> {
        const START_ENQ: usize = 1;
        let items = Atomic::null_vec(BUFFER_SIZE);
        items[0].store(*first, Ordering::Relaxed);

        let raw = Box::into_raw(Box::new(Self {
            enqidx: CachePadded::new(AtomicUsize::new(START_ENQ)),
            deqidx: CachePadded::new(AtomicUsize::new(0)),
            items,
            next: Atomic::null(),
        }));

        unsafe { Shared::from_ptr(raw) }
    }

    #[inline]
    fn cas_next<'s, S>(
        &self,
        current: Shared<Node<T>, NullTag, NullTag, 0, 0>,
        new: Shared<Node<T>, NullTag, NullTag, 0, 0>,
        shield: &S,
    ) -> bool
    where
        S: Shield<'s>,
    {
        self.next
            .compare_exchange(current, new, Ordering::SeqCst, Ordering::SeqCst, shield)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use crate::other::Queue;

    #[test]
    fn try_add_few_poll() {
        let new_queue = Queue::new();

        for _i in 0..20 {
            let found = new_queue.push(200);
        }

        let shield = new_queue.get_shield();

        for _i in 0..20 {
            let found = new_queue.pop(&shield);
            assert_eq!(found.unwrap(), &200);
        }
    }

    #[test]
    fn try_add_many_poll() {
        let new_queue = Queue::new();

        const RUNS: usize = 2_000;

        for _i in 0..RUNS {
            let found = new_queue.push(200);
        }
        let shield = new_queue.get_shield();

        for i in 0..RUNS {
            let found = new_queue.pop(&shield);
            assert_eq!(found.unwrap(), &200);
        }
    }
}
