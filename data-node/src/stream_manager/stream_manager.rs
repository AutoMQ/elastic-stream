use std::{
    collections::{hash_map::Entry, HashMap},
    rc::Rc,
};

use log::{error, info, trace, warn};
use model::range::RangeMetadata;
use store::ElasticStore;

use crate::error::ServiceError;

use super::{fetcher::Fetcher, range::Range, stream::Stream};

pub(crate) struct StreamManager {
    streams: HashMap<i64, Stream>,

    fetcher: Fetcher,

    store: Rc<ElasticStore>,
}

impl StreamManager {
    pub(crate) fn new(fetcher: Fetcher, store: Rc<ElasticStore>) -> Self {
        Self {
            streams: HashMap::new(),
            fetcher,
            store,
        }
    }

    pub(crate) async fn start(&mut self) -> Result<(), ServiceError> {
        let mut bootstrap = false;
        if let Fetcher::PlacementClient { .. } = &self.fetcher {
            bootstrap = true;
        }

        if bootstrap {
            self.bootstrap().await?;
        }
        Ok(())
    }

    /// Bootstrap all stream ranges that are assigned to current data node.
    ///
    /// # Panic
    /// If failed to access store to acquire max offset of the stream with mutable range.
    async fn bootstrap(&mut self) -> Result<(), ServiceError> {
        let ranges = self.fetcher.bootstrap().await?;

        for range in ranges {
            let entry = self.streams.entry(range.stream_id());
            match entry {
                Entry::Occupied(mut occupied) => {
                    occupied.get_mut().create_range(range)?;
                }
                Entry::Vacant(vacant) => {
                    let metadata = self.fetcher.stream(range.stream_id() as u64).await.expect(
                        "Failed to fetch stream metadata from placement manager during bootstrap",
                    );
                    let mut stream = Stream::new(metadata);
                    stream.create_range(range)?;
                    vacant.insert(stream);
                }
            }
        }
        Ok(())
    }

    /// Create a new range for the specified stream.
    pub(crate) async fn create_range(&mut self, range: RangeMetadata) -> Result<(), ServiceError> {
        info!("Create range={:?}", range);
        if let Some(stream) = self.streams.get_mut(&range.stream_id()) {
            stream.create_range(range)
        } else {
            Err(ServiceError::NotFound("Service not found".to_owned()))
        }
    }

    pub(crate) fn commit(&mut self, stream_id: i64, offset: u64) -> Result<(), ServiceError> {
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.commit(offset);
        }

        Ok(())
    }

    pub(crate) async fn seal(&mut self, range: &mut RangeMetadata) -> Result<(), ServiceError> {
        if let Some(stream) = self.streams.get_mut(&range.stream_id()) {
            stream.seal(range)
        } else {
            Err(ServiceError::NotFound("Stream not found".to_owned()))
        }
    }

    /// Get a stream by id.
    ///
    /// # Arguments
    /// `stream_id` - The id of the stream.
    ///
    /// # Returns
    /// The stream if it exists, otherwise `None`.
    pub(crate) fn get_stream(&self, stream_id: i64) -> Option<&Stream> {
        self.streams.get(&stream_id)
    }

    /// Get `StreamRange` of the given stream_id and offset.
    ///
    /// # Arguments
    /// `stream_id` - The ID of the stream.
    /// `offset` - The logical offset, starting from which to fetch records.
    ///
    /// # Returns
    /// The `StreamRange` if there is one.
    ///
    /// # Note
    /// We need to update `limit` of the returning range if it is mutable.
    pub fn stream_range_of(&self, stream_id: i64, offset: u64) -> Option<Range> {
        if let Some(stream) = self.get_stream(stream_id) {
            stream.range_of(offset)
        } else {
            None
        }
    }
}
