pub mod leitio;
pub use crate::leitio::Leitio;

#[cfg(test)]
mod tests {
    use crate::Leitio;

    #[test]
    fn it_works() {
        let _new_queue: Leitio<usize> = Leitio::new();
    }

    #[test]
    fn try_get_none() {
        let new_queue: Leitio<usize> = Leitio::new();
        let shield = new_queue.get_shield();
        let found = new_queue.pop(&shield);

        assert!(found.is_none())
    }

    #[test]
    fn try_insert() {
        let new_queue: Leitio<usize> = Leitio::new();
        let shield = new_queue.get_shield();
        let found = new_queue.pop(&shield);

        assert!(found.is_none())
    }

    #[test]
    fn try_add() {
        let new_queue: Leitio<usize> = Leitio::new();
        new_queue.push(200);
    }

    #[test]
    fn try_add_poll() {
        let new_queue: Leitio<usize> = Leitio::new();
        new_queue.push(200);

        let shield = new_queue.get_shield();
        let found = new_queue.pop(&shield);

        assert_eq!(found.unwrap(), &200);
    }

    #[test]
    fn try_add_many_poll() {
        let new_queue: Leitio<usize> = Leitio::new();

        for _i in 0..20 {
            let _found = new_queue.push(200);
        }

        for _i in 0..20 {
            let shield = new_queue.get_shield();
            let found = new_queue.pop(&shield);
            assert_eq!(found.unwrap(), &200);
        }
    }
}
