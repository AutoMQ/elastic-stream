use std::{cmp::Reverse, collections::BinaryHeap};

pub(crate) struct AppendWindow {
    /// Next offset to commit.
    ///
    /// Assume the mutable range of the stream is `[n, -1)`, we are safe to acknowledge records whose
    /// index fall into `[n, commit)`. Note `commit` is exclusive.  
    pub(crate) commit: u64,

    /// Next offset to allocate.
    pub(crate) next: u64,

    inflight: BinaryHeap<Reverse<u64>>,

    completed: BinaryHeap<Reverse<u64>>,
}

impl AppendWindow {
    pub(crate) fn new(next: u64) -> Self {
        Self {
            commit: next,
            next,
            inflight: BinaryHeap::new(),
            completed: BinaryHeap::new(),
        }
    }

    pub(crate) fn alloc_slot(&mut self) -> u64 {
        let offset = self.next;
        self.inflight.push(Reverse(offset));
        self.next += 1;
        offset
    }

    pub(crate) fn alloc_batch_slots(&mut self, batch_size: usize) -> u64 {
        let offset = self.next;
        for i in 0..batch_size {
            self.inflight.push(Reverse(offset + i as u64));
            self.next += 1;
        }
        offset
    }

    pub(crate) fn ack(&mut self, offset: u64) {
        self.completed.push(Reverse(offset));
        loop {
            if let Some(min) = self.completed.peek() {
                if let Some(offset) = self.inflight.peek() {
                    if min.0 > offset.0 {
                        // A prior record write has not yet been acknowledged.
                        break;
                    }

                    debug_assert_eq!(self.commit, offset.0);
                }
            } else {
                break;
            }

            let _ = self.completed.pop();
            if let Some(offset) = self.inflight.pop() {
                self.commit = offset.0 + 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppendWindow;
    use std::error::Error;

    #[test]
    fn test_alloc_slot() -> Result<(), Box<dyn Error>> {
        let mut window = AppendWindow::new(0);
        const TOTAL: u64 = 16;
        for i in 0..TOTAL {
            let slot = window.alloc_slot();
            assert_eq!(i, slot);
        }

        assert_eq!(TOTAL as usize, window.inflight.len());

        Ok(())
    }

    #[test]
    fn test_ack() -> Result<(), Box<dyn Error>> {
        let mut window = AppendWindow::new(0);
        const TOTAL: u64 = 16;
        for i in 0..TOTAL {
            let slot = window.alloc_slot();
            assert_eq!(i, slot);
        }

        for i in 0..TOTAL {
            window.ack(i);
            assert_eq!((TOTAL - i - 1) as usize, window.inflight.len());
            assert!(window.completed.is_empty());
            assert_eq!(i + 1, window.commit);
        }

        Ok(())
    }

    #[test]
    fn test_ack_out_of_order() -> Result<(), Box<dyn Error>> {
        let mut window = AppendWindow::new(0);
        const TOTAL: u64 = 16;
        for i in 0..TOTAL {
            let slot = window.alloc_slot();
            assert_eq!(i, slot);
        }

        for i in (0..TOTAL).rev() {
            window.ack(i);
            if i > 0 {
                assert_eq!(window.inflight.len(), TOTAL as usize);
                continue;
            }
            assert!(window.inflight.is_empty());
            assert!(window.completed.is_empty());
        }
        assert_eq!(TOTAL, window.commit);

        Ok(())
    }
}
