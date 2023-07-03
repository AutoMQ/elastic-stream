pub(crate) mod fetcher;
pub(crate) mod manager;
pub(crate) mod range;
pub(crate) mod stream;
pub(crate) mod window;

use self::stream::Stream;
use crate::error::ServiceError;
#[cfg(test)]
use mockall::automock;
use model::range::RangeMetadata;

#[cfg_attr(test, automock)]
pub(crate) trait StreamManager {
    async fn start(&mut self) -> Result<(), ServiceError>;

    /// Create a new range for the specified stream.
    fn create_range(&mut self, range: RangeMetadata) -> Result<(), ServiceError>;

    fn commit(
        &mut self,
        stream_id: i64,
        range_index: i32,
        offset: u64,
        last_offset_delta: u32,
        bytes_len: u32,
    ) -> Result<(), ServiceError>;

    fn seal(&mut self, range: &mut RangeMetadata) -> Result<(), ServiceError>;

    /// Get a stream by id.
    ///
    /// # Arguments
    /// `stream_id` - The id of the stream.
    ///
    /// # Returns
    /// The stream if it exists, otherwise `None`.
    fn get_stream<'a>(&'a mut self, stream_id: i64) -> Option<&'a mut Stream>;

    fn get_range<'a>(&'a mut self, stream_id: i64, index: i32) -> Option<&'a mut range::Range>;
}
