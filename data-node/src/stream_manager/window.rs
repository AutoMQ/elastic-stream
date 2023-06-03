use std::{collections::{HashMap, BTreeMap}, cmp};

use log::trace;
use model::Batch;
use tokio::sync::oneshot;

use crate::error::ServiceError;

/// Append Request Window ensures append requests of a stream range are dispatched to store in order.
///
/// Note that `Window` is intended to be used by a thread-per-core scenario and is not thread-safe.
#[derive(Debug)]
pub(crate) struct Window {
    /// The barrier offset
    next: u64,

    /// Submitted request offset to batch size.
    submitted: BTreeMap<u64, u32>,

    /// A queue of requests that are waiting for prior requests to be completed.
    queue: HashMap<u64, oneshot::Sender<()>>,
}

impl Window {
    pub(crate) fn new(next: u64) -> Self {
        Self {
            next,
            submitted: BTreeMap::new(),
            queue: HashMap::new(),
        }
    }

    pub fn next(&self) -> u64 {
        self.next
    }

    pub(crate) fn reset_next(&mut self, next: u64) {
        self.next = next;
    }

    /// Await append requests to be ready for dispatching.
    ///
    /// # Arguments
    /// * `request` - the request to be dispatched.
    ///
    /// # Return
    /// `Ok` if the request is ready to be dispatched, `Err` any error occurs.
    pub(crate) async fn wait_to_go<R>(&mut self, request: &R) -> Result<(), ServiceError>
    where
        R: Batch + Ord,
    {
        if request.offset() < self.next {
            // A retry request on a committed offset.
            // This is possible when the client does not
            // receive the response of the request due to network failure.
            return Err(ServiceError::OffsetRepeated);
        }

        if request.offset() == self.next {
            // Expected request to be dispatched, just advance the next offset and go.
            self.next += request.len() as u64;
            self.submitted.insert(request.offset(), request.len());
            return Ok(());
        }

        // A further request that should be queued to wait for the prior requests to be completed.
        let (tx, rx) = oneshot::channel();
        self.queue.insert(request.offset(), tx);
        trace!(
            "Request queued, offset={}, len={}",
            request.offset(),
            request.len()
        );
        // Wait the channel to be notified.
        match rx.await {
            Ok(()) => Ok(()),
            Err(e) => Err(ServiceError::Internal(e.to_string())),
        }
    }

    /// Commits the offset of current completed request, and wake up the next request if possible.
    ///
    /// # Arguments
    /// * `offset` - the offset of current completed request.
    ///
    /// # Return
    /// * the next offset to be committed.
    pub(crate) fn commit(&mut self, offset: u64) -> u64 {
        let mut res = offset;

        // Drain the submitted requests in ascending key order
        self.submitted
            .drain_filter(|k, _| k <= &offset)
            .for_each(|(offset, len)| {
                if offset + len as u64 > res {
                    res = offset + len as u64;
                }

                // Try to wake up the subsequent request if exists.
                if let Some(tx) = self.queue.remove(&res) {
                    trace!("Request dequeued, offset={}, len={}", res, len);
                    let _ = tx.send(());
                }
            });
        
        // To avoid rollback the next offset, keep the larger one.
        self.next = cmp::max(self.next, res);
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
}
