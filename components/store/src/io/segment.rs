use std::{
    cmp::Ordering,
    collections::{HashMap, VecDeque},
    fmt::Display,
    fs::{File, OpenOptions},
    os::{
        fd::{AsRawFd, FromRawFd, IntoRawFd, RawFd},
        unix::prelude::{FileExt, OpenOptionsExt},
    },
    path::Path,
    time::SystemTime,
};

use bytes::{BufMut, BytesMut};
use derivative::Derivative;
use io_uring::{opcode, squeue, types};
use nix::fcntl;
use slog::{debug, error, info, trace, warn, Logger};

use crate::{
    error::StoreError,
    io::{record::RecordType, CRC32C},
    option::WalPath,
};

use super::{block_cache::BlockCache, buf::AlignedBufWriter, record::RECORD_PREFIX_LENGTH};

// CRC(4B) + length(3B) + Type(1B) + earliest_record_time(8B) + latest_record_time(8B)
pub(crate) const FOOTER_LENGTH: u64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimeRange {
    pub(crate) begin: SystemTime,
    pub(crate) end: Option<SystemTime>,
}

impl TimeRange {
    pub(crate) fn new(begin: SystemTime) -> Self {
        Self { begin, end: None }
    }
}

/// `LogSegment` consists of fixed length blocks, which are groups of variable-length records.
///
/// The writer writes and reader reads in chunks of blocks. `BlockSize` are normally multiple of the
/// underlying storage block size, which is medium and cloud-vendor specific. By default, `BlockSize` is
/// `256KiB`.
#[derive(Derivative)]
#[derivative(PartialEq, Eq, Debug)]
pub(crate) struct LogSegment {
    /// Log segment offset in bytes, it's the absolute offset in the whole WAL.
    pub(crate) offset: u64,

    /// Fixed log segment file size
    /// offset + size = next log segment start offset
    pub(crate) size: u64,

    /// Position where this log segment has been written.
    /// It's a relative position in this log segment.
    pub(crate) written: u64,

    /// Consists of the earliest record time and the latest record time.
    pub(crate) time_range: Option<TimeRange>,

    /// The block cache layer on top of the log segment, is used to
    /// cache the most recently read or write blocks depending on the cache strategy.
    #[derivative(PartialEq = "ignore")]
    pub(crate) block_cache: BlockCache,

    /// The status of the log segment.
    pub(crate) status: Status,

    /// The path of the log segment, if the fd is a file descriptor.
    pub(crate) path: String,

    /// The underlying descriptor of the log segment.
    /// Currently, it's a file descriptor with `O_DIRECT` flag.
    pub(crate) sd: Option<SegmentDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SegmentDescriptor {
    /// The underlying storage medium of the log segment.
    pub(crate) medium: Medium,

    /// The raw file descriptor of the log segment.
    /// It's a file descriptor or a block device descriptor.
    pub(crate) fd: RawFd,

    /// The base address of the log segment, always zero if the fd is a file descriptor.
    pub(crate) base_ptr: u64,
}

/// Write-ahead-log segment file status.
///
/// `Status` indicates the opcode allowed on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    // Once a `LogSegmentFile` is constructed, there is no file in fs yet.
    // Need to `open` with `O_CREATE` flag.
    OpenAt,

    // Given `LogSegmentFile` are fixed in length, it would accelerate IO performance if space is pre-allocated.
    Fallocate64,

    // Now the segment file is ready for read/write.
    ReadWrite,

    // Once the segment file is full, it turns immutable, thus, read-only.
    Read,

    // When data in the segment file expires and ref-count turns `0`, FD shall be closed and the file deleted.
    Close,

    // Delete the file to reclaim disk space.
    UnlinkAt,
}

// TODO: a better display format is needed.
impl Display for LogSegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LogSegment {{ offset: {}, size: {}, written: {}, time_range: {:?} }}",
            self.offset, self.size, self.written, self.time_range
        )
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::OpenAt => {
                write!(f, "open")
            }
            Self::Fallocate64 => {
                write!(f, "fallocate")
            }
            Self::ReadWrite => {
                write!(f, "read/write")
            }
            Self::Read => {
                write!(f, "read")
            }
            Self::Close => {
                write!(f, "close")
            }
            Self::UnlinkAt => {
                write!(f, "unlink")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Medium {
    Ssd,
    Hdd,
    S3,
}

impl LogSegment {
    pub(crate) fn new(offset: u64, size: u64, path: String) -> Self {
        Self {
            offset,
            size,
            written: 0,
            time_range: None,
            block_cache: BlockCache::new(offset, 4096),
            sd: None,
            status: Status::OpenAt,
            path,
        }
    }

    pub(crate) fn format(offset: u64) -> String {
        format!("{:0>20}", offset)
    }

    pub(crate) fn parse_offset(path: &Path) -> Option<u64> {
        let file_name = path.file_name()?;
        let file_name = file_name.to_str()?;
        file_name.parse::<u64>().ok()
    }

    pub(crate) fn open(&mut self) -> Result<(), StoreError> {
        if self.sd.is_some() {
            return Ok(());
        }

        // Open the file for direct read/write
        let mut opts = OpenOptions::new();
        let file = opts
            .create(true)
            .read(true)
            .write(true)
            .custom_flags(libc::O_DIRECT)
            .open(Path::new(&self.path))?;
        let metadata = file.metadata()?;

        let mut sd_status = Status::OpenAt;

        if self.size != metadata.len() {
            debug_assert!(0 == metadata.len(), "LogSegmentFile is corrupted");
            fcntl::fallocate(
                file.as_raw_fd(),
                fcntl::FallocateFlags::empty(),
                0,
                self.size as libc::off_t,
            )
            .map_err(|errno| StoreError::System(errno as i32))?;
            sd_status = Status::ReadWrite;
        } else {
            // We assume the log segment file is read-only. The recovery/apply procedure would update status accordingly.
            sd_status = Status::Read;
        }

        self.sd = Some(SegmentDescriptor {
            medium: Medium::Ssd,
            fd: file.into_raw_fd(),
            base_ptr: 0,
        });

        // TODO: read time_range from meta-blocks

        Ok(())
    }

    pub fn is_full(&self) -> bool {
        self.written >= self.size
    }

    pub(crate) fn can_hold(&self, payload_length: u64) -> bool {
        self.status != Status::Read
            && self.written + RECORD_PREFIX_LENGTH + payload_length + FOOTER_LENGTH <= self.size
    }

    pub(crate) fn append_footer(
        &mut self,
        writer: &mut AlignedBufWriter,
    ) -> Result<(), StoreError> {
        let padding_length = self.size - self.written - RECORD_PREFIX_LENGTH - 8 - 8;
        let length_type: u32 = RecordType::Zero.with_length(padding_length as u32 + 8 + 8);
        let earliest: u64 = 0;
        let latest: u64 = 0;

        // Fill padding with 0

        let mut buf = BytesMut::with_capacity(padding_length as usize + 16);
        if padding_length > 0 {
            buf.resize(padding_length as usize, 0);
        }
        buf.put_u64(earliest);
        buf.put_u64(latest);
        let buf = buf.freeze();
        let crc: u32 = CRC32C.checksum(&buf[..]);
        writer.write_u32(crc)?;
        writer.write_u32(length_type)?;
        writer.write(&buf[..=padding_length as usize])?;

        Ok(())
    }

    pub(crate) fn remaining(&self) -> u64 {
        if Status::ReadWrite != self.status {
            return 0;
        } else {
            debug_assert!(self.size >= self.written);
            return self.size - self.written;
        }
    }

    pub(crate) fn append_full_record(
        &self,
        writer: &mut AlignedBufWriter,
        payload: &[u8],
    ) -> Result<(), StoreError> {
        let crc = CRC32C.checksum(payload);
        let length_type = RecordType::Full.with_length(payload.len() as u32);
        writer.write_u32(crc)?;
        writer.write_u32(length_type)?;
        writer.write(payload)?;
        Ok(())
    }
}

impl Drop for LogSegment {
    fn drop(&mut self) {
        // Close these open FD
        if let Some(sd) = self.sd.as_ref() {
            let _file = unsafe { File::from_raw_fd(sd.fd) };
        }
    }
}

impl PartialOrd for LogSegment {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let this = Path::new(&self.path);
        let that = Path::new(&other.path);
        let lhs = match this.file_name() {
            Some(name) => name,
            None => return None,
        };
        let rhs = match that.file_name() {
            Some(name) => name,
            None => return None,
        };

        lhs.partial_cmp(rhs)
    }
}

impl Ord for LogSegment {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.partial_cmp(other) {
            Some(res) => res,
            None => {
                unreachable!("Should not reach here");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LogSegment;

    #[test]
    fn test_format_number() {
        assert_eq!(
            LogSegment::format(0).len(),
            LogSegment::format(std::u64::MAX).len()
        );
    }
}
