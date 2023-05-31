use std::collections::{BinaryHeap, HashMap};

use log::trace;
use model::Batch;

/// Append Request Window ensures append requests of a stream range are dispatched to store in order.
///
/// Note that `Window` is intended to be used by a thread-per-core scenario and is not thread-safe.
#[derive(Debug)]
pub(crate) struct Window<R> {
    next: u64,

    requests: BinaryHeap<R>,

    /// Submitted request offset to batch size.
    submitted: HashMap<u64, u32>,
}

impl<R> Window<R>
where
    R: Batch + Ord,
{
    pub(crate) fn new(next: u64) -> Self {
        Self {
            next,
            requests: BinaryHeap::new(),
            submitted: HashMap::new(),
        }
    }

    pub fn next(&self) -> u64 {
        self.next
    }

    pub(crate) fn reset_next(&mut self, next: u64) {
        self.next = next;
    }

    pub(crate) fn fast_forward(&mut self, request: &R) -> bool {
        if request.offset() < self.next {
            trace!(
                "Retry request tolerated, offset={}, len={}",
                request.offset(),
                request.len()
            );
            return true;
        } else if request.offset() == self.next {
            self.next += request.len() as u64;
            self.submitted.insert(request.offset(), request.len());
            return true;
        }
        false
    }

    pub(crate) fn push(&mut self, request: R) {
        self.requests.push(request);
    }

    pub(crate) fn pop(&mut self) -> Option<R> {
        if let Some(request) = self.requests.peek() {
            if request.offset() == self.next {
                self.next = request.offset() + request.len() as u64;
                self.submitted.insert(request.offset(), request.len());
                return self.requests.pop();
            }
        }
        None
    }

    pub(crate) fn commit(&mut self, offset: u64) -> u64 {
        let mut res = offset;
        self.submitted
            .drain_filter(|k, _| k <= &offset)
            .for_each(|(offset, len)| {
                if offset + len as u64 > res {
                    res = offset + len as u64;
                }
            });
        res
    }
}

#[cfg(test)]
mod tests {
    use model::Batch;
    use std::{cmp::Ordering, error::Error};

    #[derive(Debug)]
    struct Foo {
        offset: u64,
        len: u32,
    }

    impl Batch for Foo {
        fn offset(&self) -> u64 {
            self.offset
        }

        fn len(&self) -> u32 {
            self.len
        }
    }

    impl PartialEq for Foo {
        fn eq(&self, other: &Self) -> bool {
            self.offset == other.offset
        }
    }

    impl PartialOrd for Foo {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            other.offset.partial_cmp(&self.offset)
        }
    }

    impl Eq for Foo {}

    impl Ord for Foo {
        fn cmp(&self, other: &Self) -> Ordering {
            other.offset.cmp(&self.offset)
        }
    }

    impl Foo {
        fn new(offset: u64) -> Self {
            Self { offset, len: 2 }
        }
    }

    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        let mut window = super::Window::new(0);
        let foo1 = Foo::new(0);
        assert!(window.fast_forward(&foo1));
        assert_eq!(2, window.next());
        Ok(())
    }

    #[test]
    fn test_push_pop() -> Result<(), Box<dyn Error>> {
        let mut window = super::Window::new(0);
        let foo1 = Foo::new(0);
        let foo2 = Foo::new(2);

        assert_eq!(false, window.fast_forward(&foo2));
        window.push(foo2);
        assert_eq!(None, window.pop());
        assert_eq!(true, window.fast_forward(&foo1));
        assert_eq!(Some(Foo::new(2)), window.pop());
        assert_eq!(4, window.next());
        assert!(window.requests.is_empty());

        window.reset_next(0);
        let foo1 = Foo::new(0);
        let foo2 = Foo::new(2);
        window.push(foo2);
        window.push(foo1);
        assert_eq!(Some(Foo::new(0)), window.pop());
        assert_eq!(Some(Foo::new(2)), window.pop());
        assert_eq!(4, window.next());
        assert!(window.requests.is_empty());

        Ok(())
    }

    #[test]
    fn test_commit() -> Result<(), Box<dyn Error>> {
        let mut window = super::Window::new(0);
        let foo1 = Foo::new(0);
        let foo2 = Foo::new(2);

        if !window.fast_forward(&foo2) {
            window.push(foo2);
        }

        // After fast-forward, an inflight batch entry is inserted.
        assert!(window.fast_forward(&foo1));
        // When commit, the offset should be amended if there is a corresponding inflight batch entry.
        // After the commit , the entry will be removed.
        assert_eq!(2, window.commit(0));
        assert_eq!(0, window.commit(0));

        // If there is no inflight batch entry, the commit offset should not be amended.
        assert_eq!(2, window.commit(2));

        // After popping a request, an inflight batch entry is inserted.
        window.pop();
        assert_eq!(4, window.commit(2));

        // There is no inflight batch entry, so the commit offset should not be amended.
        assert_eq!(4, window.commit(4));
        Ok(())
    }
}
