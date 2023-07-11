use config::ObjectStorageConfig;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
use tokio::{
    sync::mpsc::{self, UnboundedReceiver},
    time::sleep,
};

use crate::{range_fetcher::RangeFetcher, range_offload::RangeOffload, ObjectManager, RangeKey};

pub trait RangeAccumulator {
    fn accumulate(&self, records_size: u32) -> (i32, bool);

    fn try_flush(&self, max_duration: Duration) -> i32;

    fn try_offload_part(&self) -> i32;
}

pub struct DefaultRangeAccumulator {
    size: RefCell<u32>,
    tx: mpsc::UnboundedSender<EventKind>,
    object_size: u32,
    part_size: u32,
    timestamp: RefCell<Instant>,
}

impl RangeAccumulator for DefaultRangeAccumulator {
    /// Accumulate new record, trigger batch offload when the buffer is large than object size.
    /// return (
    ///     range accumulator buffer size change,
    ///     whether the buffer length is large than part size.
    /// )
    fn accumulate(&self, records_size: u32) -> (i32, bool) {
        let mut size = self.size.borrow_mut();
        if *size + records_size >= self.object_size {
            let old_size = *size;
            // trigger offload when there unloaded records size is large than object_size.
            *size = 0;
            self.timestamp.replace(Instant::now());
            let _ = self.tx.send(EventKind::ObjectFull);
            (-(old_size as i32), false)
        } else {
            *size += records_size;
            (records_size as i32, *size >= self.part_size)
        }
    }

    /// Try flush when last flush is too long ago.
    /// return (
    ///     range accumulator buffer size change
    /// )
    fn try_flush(&self, max_duration: Duration) -> i32 {
        let mut timestamp = self.timestamp.borrow_mut();
        let mut size = self.size.borrow_mut();
        if *size != 0 && timestamp.elapsed() > max_duration {
            let old_size = *size;
            *timestamp = Instant::now();
            *size = 0;
            let _ = self.tx.send(EventKind::TimeExpired);
            -(old_size as i32)
        } else {
            0
        }
    }

    /// Try offload part when the buffer length is large than part size.
    /// return (
    ///     range accumulator buffer size change
    /// )
    fn try_offload_part(&self) -> i32 {
        let mut size = self.size.borrow_mut();
        if *size >= self.part_size {
            let old_size = *size;
            *size = 0;
            let _ = self.tx.send(EventKind::PartFull);
            -(old_size as i32)
        } else {
            0
        }
    }
}

impl DefaultRangeAccumulator {
    pub fn new<F: RangeFetcher + 'static, M: ObjectManager + 'static>(
        range: RangeKey,
        start_offset: u64,
        range_fetcher: Rc<F>,
        config: ObjectStorageConfig,
        range_offload: Rc<RangeOffload<M>>,
    ) -> Self {
        // TODO: implement drop to close the read_loop
        let (tx, rx) = mpsc::unbounded_channel();

        Self::read_loop(
            range,
            start_offset,
            config.object_size,
            rx,
            range_fetcher,
            range_offload,
        );

        DefaultRangeAccumulator {
            size: RefCell::new(0),
            tx,
            object_size: config.object_size,
            part_size: config.part_size,
            timestamp: RefCell::new(Instant::now()),
        }
    }

    fn read_loop<F: RangeFetcher + 'static, M: ObjectManager + 'static>(
        range: RangeKey,
        start_offset: u64,
        object_size: u32,
        mut rx: UnboundedReceiver<EventKind>,
        range_fetcher: Rc<F>,
        range_offload: Rc<RangeOffload<M>>,
    ) {
        tokio_uring::spawn(async move {
            let stream_id = range.stream_id;
            let range_index = range.range_index;
            let mut next_offset = start_offset;
            while let Some(event) = rx.recv().await {
                let mut force_flush = false;
                if let EventKind::TimeExpired = event {
                    force_flush = true;
                }
                loop {
                    match range_fetcher
                        .fetch(
                            stream_id,
                            range_index,
                            next_offset,
                            u64::MAX,
                            object_size * 3 / 2,
                        )
                        .await
                    {
                        Ok(records) => {
                            if records.payload.is_empty() {
                                // read to end
                                break;
                            }
                            next_offset = range_offload.write(next_offset, records.payload);
                        }
                        Err(e) => {
                            log::error!(
                                "fetch range{stream_id}#{range_index} failed, retry later, {}",
                                e
                            );
                            sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    }
                }
                if force_flush {
                    range_offload.flush();
                }
            }
        });
    }
}

enum EventKind {
    ObjectFull,
    PartFull,
    TimeExpired,
}
