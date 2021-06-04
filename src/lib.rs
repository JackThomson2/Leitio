#![feature(allocator_api)]

pub mod michael_scott;
pub mod other;

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    usize,
};

use flize::{Atomic, CachePadded, Collector, NullTag, Shared, Shield, ThinShield};

pub struct Leitio<D: Clone> {
    cells: Vec<Atomic<D, NullTag, NullTag, 0, 0>>,
    start: CachePadded<AtomicUsize>,
    end: CachePadded<AtomicUsize>,
    collector: Collector,
    size: usize,
}

impl<D: Clone> Leitio<D> {
    #[inline]
    pub fn new(size: usize) -> Self {
        let cells = Atomic::null_vec(size);

        Self {
            cells,
            start: CachePadded::new(AtomicUsize::new(0)),
            end: CachePadded::new(AtomicUsize::new(0)),
            collector: Collector::new(),
            size,
        }
    }

    #[inline]
    pub fn get_shield(&self) -> ThinShield {
        self.collector.thin_shield()
    }

    #[inline]
    pub fn try_insert(&self, data: &D) -> bool {
        loop {
            let end = self.end.fetch_add(1, Ordering::SeqCst);

            // We've run out of space
            if self.start.load(Ordering::SeqCst) > end {
                return false;
            }

            let entry_point = unsafe { self.cells.get_unchecked(end) };

            let raw = Box::into_raw(Box::new(data.clone()));
            let shared = unsafe { Shared::from_ptr(raw) };

            entry_point.store(shared, Ordering::SeqCst);

            return true;
        }
    }

    #[inline]
    pub fn try_get<'s, S>(&self, shield: &'s S) -> Option<&'s D>
    where
        S: Shield<'s>,
    {
        loop {
            let looking_at = self.start.fetch_add(1, Ordering::SeqCst);
            let end = self.end.load(Ordering::Acquire);

            // We have nothing to get yet..
            if looking_at == end {
                return None;
            }

            let to_load = unsafe { self.cells.get_unchecked(looking_at) };
            let loaded = to_load.load(Ordering::Acquire, shield);

            // Someone has already grabbed it
            if loaded.is_null() {
                continue;
            }

            unsafe {
                match to_load.compare_exchange_weak(
                    loaded,
                    Shared::null(),
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                    shield,
                ) {
                    Ok(res) => {
                        return Some(res.as_ref_unchecked());
                    }
                    Err(_err) => continue,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Leitio;

    #[test]
    fn it_works() {
        let _new_queue: Leitio<usize> = Leitio::new(100);
    }

    #[test]
    fn try_get_none() {
        let new_queue: Leitio<usize> = Leitio::new(100);
        let shield = new_queue.get_shield();
        let found = new_queue.try_get(&shield);

        assert!(found.is_none())
    }

    #[test]
    fn try_insert() {
        let new_queue: Leitio<usize> = Leitio::new(100);
        let shield = new_queue.get_shield();
        let found = new_queue.try_get(&shield);

        assert!(found.is_none())
    }

    #[test]
    fn try_add() {
        let new_queue: Leitio<usize> = Leitio::new(100);
        let found = new_queue.try_insert(&200);

        assert!(found)
    }

    #[test]
    fn try_add_poll() {
        let new_queue: Leitio<usize> = Leitio::new(100);
        let found = new_queue.try_insert(&200);

        assert!(found);

        let shield = new_queue.get_shield();
        let found = new_queue.try_get(&shield);

        assert_eq!(found.unwrap(), &200);
    }

    #[test]
    fn try_add_many_poll() {
        let new_queue: Leitio<usize> = Leitio::new(100);

        for _i in 0..20 {
            let found = new_queue.try_insert(&200);
            assert!(found);
        }

        for _i in 0..20 {
            let shield = new_queue.get_shield();
            let found = new_queue.try_get(&shield);
            assert_eq!(found.unwrap(), &200);
        }
    }
}
