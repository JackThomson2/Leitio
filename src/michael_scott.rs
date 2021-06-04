use std::{mem::MaybeUninit, sync::atomic::Ordering};

use flize::{Atomic, Collector, NullTag, Shared, Shield, ThinShield};

pub struct MichaelScottQueue<T> {
    head: Atomic<Node<T>, NullTag, NullTag, 0, 0>,
    tail: Atomic<Node<T>, NullTag, NullTag, 0, 0>,
    collector: Collector,
}

impl<T> MichaelScottQueue<T> {
    #[inline]
    pub fn new() -> Self {
        let start = Node::null_node();
        Self {
            head: Atomic::new(start),
            tail: Atomic::new(start),
            collector: Collector::new(),
        }
    }

    #[inline]
    pub fn push(&self, data: T) {
        let new_node = Node::new(data);
        let shield = self.collector.thin_shield();

        loop {
            let tail = self.tail.load(Ordering::SeqCst, &shield);
            let updating = match self.tail.compare_exchange_weak(
                tail,
                new_node,
                Ordering::SeqCst,
                Ordering::Relaxed,
                &shield,
            ) {
                Ok(res) => res,
                Err(_err) => continue,
            };

            if updating.is_null() {
                panic!("This shouldn't happen");
            }

            let to_update = unsafe { updating.as_ref_unchecked() };
            to_update.next.store(new_node, Ordering::SeqCst);
            return;
        }
    }

    #[inline]
    pub fn get_shield(&self) -> ThinShield {
        self.collector.thin_shield()
    }

    #[inline]
    pub fn pop<'s, S>(&self, shield: &'s S) -> Option<&'s T>
    where
        S: Shield<'s>,
    {
        loop {
            let head = self.head.load(Ordering::SeqCst, shield);
            // Head always there...
            let data = unsafe { head.as_ref_unchecked() };

            let next = data.next.load(Ordering::SeqCst, shield);
            // We have no items...
            if next.is_null() {
                return None;
            }

            let popped_item = unsafe { next.as_ref_unchecked() };
            match data.next.compare_exchange_weak(
                next,
                popped_item.next.load(Ordering::SeqCst, shield),
                Ordering::SeqCst,
                Ordering::Relaxed,
                shield,
            ) {
                Ok(_res) => {
                    shield.retire(move || unsafe {
                        Box::from_raw(next.as_ptr());
                    });

                    return Some(&popped_item.data);
                }
                Err(_err) => continue,
            }
        }
    }
}

pub struct Node<T> {
    data: T,
    pub next: Atomic<Node<T>, NullTag, NullTag, 0, 0>,
}

impl<T> Node<T> {
    #[inline]
    pub fn null_node<'s>() -> Shared<'s, Node<T>, NullTag, NullTag, 0, 0> {
        let raw = Box::into_raw(Box::new(Self {
            data: unsafe { MaybeUninit::uninit().assume_init() },
            next: Atomic::null(),
        }));

        unsafe { Shared::from_ptr(raw) }
    }

    #[inline]
    pub fn new<'s>(data: T) -> Shared<'s, Node<T>, NullTag, NullTag, 0, 0> {
        let raw = Box::into_raw(Box::new(Self {
            data,
            next: Atomic::null(),
        }));

        unsafe { Shared::from_ptr(raw) }
    }
}

#[cfg(test)]
mod tests {
    use crate::michael_scott::MichaelScottQueue;

    #[test]
    fn it_works() {
        let _new_queue: MichaelScottQueue<usize> = MichaelScottQueue::new();
    }

    #[test]
    fn try_get_none() {
        let new_queue: MichaelScottQueue<usize> = MichaelScottQueue::new();
        let shield = new_queue.get_shield();
        let found = new_queue.pop(&shield);

        assert!(found.is_none())
    }

    #[test]
    fn try_add() {
        let new_queue = MichaelScottQueue::new();
        new_queue.push(&200);
    }

    #[test]
    fn try_add_poll() {
        let new_queue = MichaelScottQueue::new();
        let found = new_queue.push(200);

        let shield = new_queue.get_shield();
        let found = new_queue.pop(&shield);

        assert_eq!(found.unwrap(), &200);
    }

    #[test]
    fn try_add_many_poll() {
        let new_queue = MichaelScottQueue::new();

        for _i in 0..20 {
            let found = new_queue.push(200);
        }

        let shield = new_queue.get_shield();
        for _i in 0..20 {
            let found = new_queue.pop(&shield);
            assert_eq!(found.unwrap(), &200);
        }
    }
}
