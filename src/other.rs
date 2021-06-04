/// Based on https://github.com/xacrimon/flize/blob/3358915c7d13c09a04d34537869c0f380339b298/examples/queue/src/lib.rs#L1
use flize::NullTag;
use flize::{unprotected, Atomic, Collector, Shared, Shield};
use std::sync::atomic::{AtomicUsize, Ordering};

const BUFFER_SIZE: usize = 1024;

pub struct Queue<T: Clone> {
    collector: Collector,
    head: Atomic<Node<T>, NullTag, NullTag, 0, 0>,
    tail: Atomic<Node<T>, NullTag, NullTag, 0, 0>,
}

impl<T: Clone> Queue<T> {
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

    pub fn new() -> Self {
        let sentinel = Node::empty(true);

        Self {
            collector: Collector::new(),
            head: Atomic::new(sentinel),
            tail: Atomic::new(sentinel),
        }
    }

    pub fn push(&self, value: T) {
        let shield = self.collector.thin_shield();

        let new_item = Box::into_raw(Box::new(value));
        let shared = unsafe { Shared::from_ptr(new_item) };

        loop {
            let ltail = self.tail.load(Ordering::SeqCst, &shield);
            let ltailr = unsafe { ltail.as_ref_unchecked() };
            let idx = ltailr.enqidx.fetch_add(1, Ordering::SeqCst);

            if idx > BUFFER_SIZE - 1 {
                if ltail != self.tail.load(Ordering::SeqCst, &shield) {
                    continue;
                }

                let lnext = ltailr.next.load(Ordering::SeqCst, &shield);

                if lnext.is_null() {
                    let new_node = Node::new(shared, false);

                    if ltailr.cas_next(Shared::null(), new_node, &shield) {
                        self.cas_tail(ltail, new_node, &shield);
                        return;
                    }

                    shield.retire(move || unsafe {
                        Box::from_raw(new_node.as_ptr());
                    });
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

    pub fn pop(&self) -> Option<T> {
        let shield = self.collector.thin_shield();

        loop {
            let lhead = self.head.load(Ordering::SeqCst, &shield);
            let lheadr = unsafe { lhead.as_ref_unchecked() };

            if lheadr.deqidx.load(Ordering::SeqCst) >= lheadr.enqidx.load(Ordering::SeqCst)
                && lheadr.next.load(Ordering::SeqCst, &shield).is_null()
            {
                break None;
            }

            let idx = lheadr.deqidx.fetch_add(1, Ordering::SeqCst);

            if idx > BUFFER_SIZE - 1 {
                let lnext = lheadr.next.load(Ordering::SeqCst, &shield);

                if lnext.is_null() {
                    break None;
                }

                if self.cas_head(lhead, lnext, &shield) {
                    shield.retire(move || unsafe {
                        Box::from_raw(lhead.as_ptr());
                    });
                }

                continue;
            }

            let item = lheadr.items[idx].swap(Shared::null(), Ordering::SeqCst, &shield);

            if item.is_null() {
                continue;
            }

            return Some(unsafe { item.as_ref_unchecked().clone() });
        }
    }
}

impl<T: Clone> Drop for Queue<T> {
    fn drop(&mut self) {
        while let Some(_) = self.pop() {}

        unsafe {
            let shield = unprotected();
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
    enqidx: AtomicUsize,
    deqidx: AtomicUsize,
    items: Vec<Atomic<T, NullTag, NullTag, 0, 0>>,
    next: Atomic<Self, NullTag, NullTag, 0, 0>,
}

impl<T: Clone> Node<T> {
    fn empty<'a>(sentinel: bool) -> Shared<'a, Node<T>, NullTag, NullTag, 0, 0> {
        let start_enq = if sentinel { 0 } else { 1 };
        let items = Atomic::null_vec(BUFFER_SIZE);

        let raw = Box::into_raw(Box::new(Self {
            enqidx: AtomicUsize::new(start_enq),
            deqidx: AtomicUsize::new(start_enq),
            items,
            next: Atomic::null(),
        }));

        unsafe { Shared::from_ptr(raw) }
    }

    fn new<'a>(
        first: Shared<T, NullTag, NullTag, 0, 0>,
        sentinel: bool,
    ) -> Shared<'a, Node<T>, NullTag, NullTag, 0, 0> {
        let start_enq = if sentinel { 0 } else { 1 };
        let items = Atomic::null_vec(BUFFER_SIZE);
        items[0].store(first, Ordering::Relaxed);

        let raw = Box::into_raw(Box::new(Self {
            enqidx: AtomicUsize::new(start_enq),
            deqidx: AtomicUsize::new(start_enq),
            items,
            next: Atomic::null(),
        }));

        unsafe { Shared::from_ptr(raw) }
    }

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
