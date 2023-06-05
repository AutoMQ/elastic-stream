use log::{error, warn};
use std::{cmp, collections::BTreeMap};

use model::Batch;

use crate::error::ServiceError;

/// Append Request Window ensures append requests of a stream range are dispatched to store in order.
///
/// Note that `Window` is intended to be used by a thread-per-core scenario and is not thread-safe.
///
/// # Design
/// Each `Window` has two offset, `next` and `committed`.
/// * `next` is the next exact offset of the request to be dispatched.
/// * `committed` is the committed offset of the stream range.
///
/// The below rules show the relationship between `next` and `committed`:
/// * `next` is always greater than or equal to `committed`.
/// * The request with offset equal to `next` is the expected next request to be dispatched.
/// * The request with offset less than `committed` should be responded with a `ServiceError::OffsetCommitted`.
/// * The request with offset greater than `next` should be responded with a `ServiceError::OffsetOutOfOrder`.
/// * The other requests should be responded with a `ServiceError::OffsetInWindow`.
#[derive(Debug)]
pub(crate) struct Window {
    /// The barrier offset, the requests beyond this offset should be blocked.
    next: u64,

    /// The committed offset means all records prior to this offset are already persisted to store.
    committed: u64,

    /// Submitted request offset to batch size.
    submitted: BTreeMap<u64, u32>,
}

impl Window {
    pub(crate) fn new(next: u64) -> Self {
        Self {
            next,
            submitted: BTreeMap::new(),
            // The initial commit offset is the same as the next offset.
            committed: next,
        }
    }

    pub fn next(&self) -> u64 {
        self.next
    }

    pub(crate) fn reset(&mut self, offset: u64) {
        self.next = offset;
        self.committed = offset;
    }

    /// Checks the request with the given offset is ready to be dispatched.
    ///
    /// # Arguments
    /// * `request` - the request to be dispatched.
    ///
    /// # Return
    /// `Ok` if the request is ready to be dispatched, `Err` any error occurs.
    pub(crate) fn check_barrier<R>(&mut self, request: &R) -> Result<(), ServiceError>
    where
        R: Batch + Ord,
    {
        if request.offset() < self.committed {
            // A retry request on a committed offset.
            // The client could regard the request as success.
            warn!(
                "Try to append request on committed offset {}, current committed={:?}",
                request.offset(),
                self.committed
            );
            return Err(ServiceError::OffsetCommitted);
        }

        if request.offset() < self.next {
            // A retry request on a offset that is already in the write window.
            // To avoid data loss, the client should await the request to be completed and retry if necessary.
            warn!(
                "Try to append request on offset {}, which is in the write window, current next={:?}",
                request.offset(),
                self.next
            );
            return Err(ServiceError::OffsetInFlight);
        }

        if request.offset() > self.next {
            // A request on a offset that is beyond the write window.
            // The client should promise the order of the requests.
            error!(
                "Try to append request on offset {}, which is beyond the write window, current next={:?}",
                request.offset(),
                self.next
            );
            return Err(ServiceError::OffsetOutOfOrder);
        }

        self.submitted.insert(request.offset(), request.len());
        // Expected request to be dispatched, just advance the next offset and go.
        self.next += request.len() as u64;
        return Ok(());
    }

    /// Commits the request with the given offset, and wakes up the subsequent request if exists.
    ///
    /// Note that this method will be called in the bootstrap phase to init the committed and next offset.
    ///
    /// # Arguments
    /// * `offset` - the offset to be committed.
    ///
    /// # Return
    /// * the committed offset.
    pub(crate) fn commit(&mut self, offset: u64) -> u64 {
        let mut res = offset;

        debug_assert!(
            !self.submitted.is_empty(),
            "Must check-barrier prior to commit"
        );

        // TODO: below code is too defensive, we should find a more concise way to do this.
        if let Some((first_key, _len)) = self.submitted.first_key_value() {
            if *first_key != offset {
                // Unexpected commit call, the offset should be the first key of the submitted map.
                error!("Unexpected commit call, the offset should be the first key of the submitted map, offset: {}, first_key: {}", offset, first_key);
            }
        }

        // Drain the submitted requests in ascending key order, and commit all the requests before the given offset.
        // Note: in the current thread per core scenario, the requests are committed in the same order as they are submitted.
        self.submitted
            .drain_filter(|k, _| k <= &offset)
            .for_each(|(offset, len)| {
                if offset + len as u64 > res {
                    res = offset + len as u64;
                }
            });

        // To avoid rollback the commit offset, keep the larger one.
        self.committed = cmp::max(self.committed, res);
        self.committed
    }
}

#[cfg(test)]
mod tests {
    use model::Batch;
    use std::cmp::Ordering;

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
    fn test_new() {
        let mut window = super::Window::new(0);
        let foo1 = Foo::new(0);
        assert!(window.check_barrier(&foo1).is_ok());
        assert_eq!(2, window.next());
    }

    #[test]
    fn test_check_barrier() {
        let mut window = super::Window::new(0);
        let foo1 = Foo::new(0);
        let foo2 = Foo::new(2);

        // foo2 will be rejected because the offset is beyond the write window, the error is OffsetOutOfOrder.
        assert!(matches!(
            window.check_barrier(&foo2).unwrap_err(),
            super::ServiceError::OffsetOutOfOrder
        ));

        // foo1 will be accepted.
        assert!(window.check_barrier(&foo1).is_ok());
        assert_eq!(2, window.next());
        // Now, foo2 will be accepted.
        assert!(window.check_barrier(&foo2).is_ok());

        // Commit foo2, and foo1 will be committed implicitly.
        assert_eq!(4, window.commit(2));

        // Now, foo1 will be rejected because the offset is already committed, the error is OffsetCommitted.
        assert!(matches!(
            window.check_barrier(&foo1).unwrap_err(),
            super::ServiceError::OffsetCommitted
        ));

        let foo3 = Foo::new(4);
        assert!(window.check_barrier(&foo3).is_ok());

        // The in-flight request will be rejected, the error is OffsetInFlight.
        assert!(matches!(
            window.check_barrier(&foo3).unwrap_err(),
            super::ServiceError::OffsetInFlight
        ));

        assert!(window.next == 6);
        assert!(window.committed == 4);
    }
}
