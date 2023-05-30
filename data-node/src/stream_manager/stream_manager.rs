use std::{
    collections::{hash_map::Entry, HashMap},
    rc::Rc,
};

use log::info;
use model::range::RangeMetadata;
use store::{ElasticStore, Store};

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
        let ranges = self
            .fetcher
            .bootstrap(self.store.config().server.node_id as u32)
            .await?;

        for range in ranges {
            let committed = self
                .store
                .max_record_offset(range.stream_id(), range.index() as u32)
                .expect("Failed to acquire max offset of the range");
            let range_index = range.index();
            let entry = self.streams.entry(range.stream_id());
            match entry {
                Entry::Occupied(mut occupied) => {
                    occupied.get_mut().create_range(range)?;
                    if let Some(offset) = committed {
                        occupied.get_mut().reset_commit(range_index, offset);
                    }
                }
                Entry::Vacant(vacant) => {
                    let metadata = self.fetcher.describe_stream(range.stream_id() as u64).await.expect(
                        "Failed to fetch stream metadata from placement manager during bootstrap",
                    );
                    let mut stream = Stream::new(metadata);
                    stream.create_range(range)?;
                    if let Some(offset) = committed {
                        stream.reset_commit(range_index, offset);
                    }
                    vacant.insert(stream);
                }
            }
        }
        Ok(())
    }

    /// Create a new range for the specified stream.
    pub(crate) async fn create_range(&mut self, range: RangeMetadata) -> Result<(), ServiceError> {
        info!("Create range={:?}", range);

        match self.streams.entry(range.stream_id()) {
            Entry::Occupied(mut occupied) => {
                occupied.get_mut().create_range(range)?;
            }
            Entry::Vacant(vacant) => {
                let metadata = self
                    .fetcher
                    .describe_stream(range.stream_id() as u64)
                    .await?;
                let mut stream = Stream::new(metadata);
                stream.create_range(range)?;
                vacant.insert(stream);
            }
        }
        Ok(())
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
            info!(
                "Stream[id={}] is not found, fetch stream metadata from placement manager",
                range.stream_id()
            );
            let stream_metadata = self
                .fetcher
                .describe_stream(range.stream_id() as u64)
                .await?;
            let mut stream = Stream::new(stream_metadata);
            stream.create_range(range.clone())?;
            self.streams.insert(range.stream_id(), stream);
            Ok(())
        }
    }

    /// Get a stream by id.
    ///
    /// # Arguments
    /// `stream_id` - The id of the stream.
    ///
    /// # Returns
    /// The stream if it exists, otherwise `None`.
    pub(crate) fn get_stream(&mut self, stream_id: i64) -> Option<&mut Stream> {
        self.streams.get_mut(&stream_id)
    }

    pub fn get_range(&mut self, stream_id: i64, index: i32) -> Option<&mut Range> {
        if let Some(stream) = self.get_stream(stream_id) {
            stream.get_range(index)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    #[test]
    fn test_new() -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}
