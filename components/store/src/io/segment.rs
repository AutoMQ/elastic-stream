use bytes::{Buf, BufMut, BytesMut};
use derivative::Derivative;
use nix::fcntl;
use std::{
    cmp::Ordering,
    ffi::CString,
    fmt::{self, Display},
    fs::{File, OpenOptions},
    os::{
        fd::{AsRawFd, FromRawFd, IntoRawFd, RawFd},
        unix::prelude::{FileExt, OpenOptionsExt, OsStrExt},
    },
    path::Path,
    sync::Arc,
    time::SystemTime,
};

use crate::{
    error::StoreError,
    io::{buf::AlignedBuf, record::RecordType},
};

use super::{
    block_cache::{BlockCache, EntryRange},
    buf::{AlignedBufReader, AlignedBufWriter},
    record::RECORD_PREFIX_LENGTH,
};

// CRC(4B) + length(3B) + Type(1B) + earliest_record_time(8B) + latest_record_time(8B)
pub(crate) const FOOTER_LENGTH: u64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TimeRange {
    pub(crate) begin: SystemTime,
    pub(crate) end: Option<SystemTime>,
}

/// `LogSegment` consists of fixed length blocks, which are groups of variable-length records.
///
/// The writer writes and reader reads in chunks of blocks. `BlockSize` are normally multiple of the
/// underlying storage block size, which is medium and cloud-vendor specific. By default, `BlockSize` is
/// `256KiB`.
#[derive(Derivative)]
#[derivative(PartialEq, Eq, Debug)]
pub(crate) struct LogSegment {
    #[derivative(PartialEq = "ignore")]
    config: Arc<config::Configuration>,

    /// Log segment offset in bytes, it's the absolute offset in the whole WAL.
    pub(crate) wal_offset: u64,

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
    pub(crate) path: CString,

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

    // Sync the file metadata after create/fallocate
    Fsync,

    // Now the segment file is ready for read/write.
    ReadWrite,

    // Once the segment file is full, it turns immutable, thus, read-only.
    Read,

    // When data in the segment file expires and ref-count turns `0`, FD shall be closed and the file deleted.
    Close,

    // Delete the file to reclaim disk space.
    UnlinkAt,

    // The segment file is ready to reuse.
    Recycled,

    // Rename the recycled segment file.
    RenameAt,
}

impl Display for LogSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path.to_str().map_err(|_| fmt::Error)?;
        write!(
            f,
            "LogSegment {{ path: {}, offset: {}, size: {}, written: {}, time_range: {:?} }}",
            path, self.wal_offset, self.size, self.written, self.time_range
        )
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::OpenAt => {
                write!(f, "open")
            }
            Self::Fallocate64 => {
                write!(f, "fallocate")
            }
            Self::Fsync => {
                write!(f, "fsync")
            }
            Self::ReadWrite => {
                write!(f, "read|write")
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
            Self::Recycled => {
                write!(f, "recycled")
            }
            Self::RenameAt => {
                write!(f, "rename")
            }
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Medium {
    Ssd,
    Hdd,
    S3,
}

impl LogSegment {
    pub(crate) fn new(
        config: &Arc<config::Configuration>,
        offset: u64,
        size: u64,
        path: &Path,
    ) -> Result<Self, StoreError> {
        Ok(Self {
            config: Arc::clone(config),
            wal_offset: offset,
            size,
            written: 0,
            time_range: None,
            block_cache: BlockCache::new(config, offset),
            sd: None,
            status: Status::OpenAt,
            path: CString::new(path.as_os_str().as_bytes())
                .map_err(|e| StoreError::InvalidPath(e.to_string()))?,
        })
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
            .custom_flags(libc::O_RDWR | libc::O_DIRECT | libc::O_DSYNC)
            .open(Path::new(
                self.path
                    .to_str()
                    .map_err(|e| StoreError::InvalidPath(e.to_string()))?,
            ))?;
        let metadata = file.metadata()?;

        let status = if self.size != metadata.len() {
            debug_assert!(0 == metadata.len(), "LogSegmentFile is corrupted");
            fcntl::fallocate(
                file.as_raw_fd(),
                fcntl::FallocateFlags::empty(),
                0,
                self.size as libc::off_t,
            )
            .map_err(|errno| StoreError::System(errno as i32))?;
            Status::ReadWrite
        } else {
            // We assume the log segment file is read-only. The recovery/apply procedure would update status accordingly.
            Status::Read
        };

        self.status = status;
        self.sd = Some(SegmentDescriptor {
            medium: Medium::Ssd,
            fd: file.into_raw_fd(),
            base_ptr: 0,
        });

        // TODO: read time_range from meta-blocks

        Ok(())
    }

    pub fn writable(&self) -> bool {
        self.written < self.size && self.status == Status::ReadWrite
    }

    pub(crate) fn can_hold(&self, payload_length: u64) -> bool {
        self.status != Status::Read
            && self.written + RECORD_PREFIX_LENGTH + payload_length + FOOTER_LENGTH <= self.size
    }

    pub(crate) fn append_footer(
        &mut self,
        writer: &mut AlignedBufWriter,
    ) -> Result<u64, StoreError> {
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
        let crc: u32 = util::crc32::crc32(&buf[..]);
        let mut total_buf = BytesMut::with_capacity(4 + 8);
        total_buf.put_u32(crc);
        total_buf.put_u64(self.wal_offset);
        let crc = util::crc32::crc32(total_buf);
        writer.write_u32(crc)?;
        writer.write_u32(length_type)?;
        writer.write(&buf[..])?;

        self.written = self.size;

        // After the footer is written, the log segment is read-only.
        self.status = Status::Read;
        Ok(self.wal_offset + self.written)
    }

    #[allow(dead_code)]
    pub(crate) fn remaining(&self) -> u64 {
        if Status::ReadWrite != self.status {
            0
        } else {
            debug_assert!(self.size >= self.written);
            self.size - self.written
        }
    }

    pub(crate) fn append_record(
        &mut self,
        writer: &mut AlignedBufWriter,
        payload: &[u8],
    ) -> Result<u64, StoreError> {
        let crc = util::crc32::crc32(payload);
        let mut total_buf = BytesMut::with_capacity(4 + 8);
        total_buf.put_u32(crc);
        total_buf.put_u64(self.wal_offset);
        let crc = util::crc32::crc32(total_buf);
        let length_type = RecordType::Full.with_length(payload.len() as u32);
        writer.write_u32(crc)?;
        writer.write_u32(length_type)?;
        writer.write(payload)?;
        self.written += 4 + 4 + payload.len() as u64;
        Ok(self.wal_offset + self.written)
    }

    /// Reads the exact number of byte required to fill `buf` from the given relative offset of the segment.
    ///
    /// This method will try read from the block cache first. If the block is not cached,
    /// it will read from the underlying sd(SegmentDescriptor), and then cache the block.
    ///
    /// # Arguments
    /// * `buf` - The buffer to read into.
    /// * `r_offset` - The relative offset of the segment.
    ///
    /// # Note
    /// This method uses classical read-at approach to read data from the underlying file,
    /// it's designed to be used in the recovery procedure.
    pub(crate) fn read_exact_at(
        &mut self,
        buf: &mut [u8],
        r_offset: u64,
    ) -> Result<(), StoreError> {
        let entry = EntryRange::new(
            self.wal_offset + r_offset,
            buf.len() as u32,
            self.config.store.alignment,
        );

        let read_at = self.wal_offset + r_offset;
        let read_len = buf.len() as u64;

        // Store the slices of the needed data temporarily.
        let mut slice_v = vec![];

        loop {
            let buf_res = self.block_cache.try_get_entries(entry);
            match buf_res {
                Ok(Some(buf_v)) => {
                    // Copy the cached data to the given buffer.
                    // We need narrow the first and the last AlignedBuf to the actual read range
                    buf_v.into_iter().for_each(|buf_r| {
                        let mut start_pos = 0;

                        if buf_r.wal_offset < read_at {
                            start_pos = (read_at - buf_r.wal_offset) as u32;
                        }

                        let mut limit = buf_r.limit();
                        let limit_len = (buf_r.wal_offset + buf_r.limit() as u64)
                            .checked_sub(read_at + read_len);
                        if let Some(limit_len) = limit_len {
                            limit -= limit_len as usize;
                        }
                        let mut buf = buf_r.slice(start_pos as usize..limit);
                        let mut dst = vec![0_u8; buf.len()];
                        buf.copy_to_slice(&mut dst[..]);
                        slice_v.push(dst);
                    });
                    break;
                }
                Ok(None) => {
                    // Impossible, the block cache should always return a result since this is in recovery procedure.
                    return Err(StoreError::CacheError);
                }
                Err(entries) => {
                    // Cache miss, there are some entries should be read from disk.
                    let sd = self.sd.as_ref().ok_or(StoreError::NotOpened)?;

                    // Open the file with the given `fd`.
                    let file = unsafe { File::from_raw_fd(sd.fd) };

                    let try_r: Result<(), StoreError> = entries.into_iter().try_for_each(|e| {
                        if let Ok(buf) = AlignedBufReader::alloc_read_buf(
                            e.wal_offset,
                            e.len as usize,
                            self.config.store.alignment,
                        ) {
                            let file_pos = e.wal_offset - self.wal_offset;

                            file.read_exact_at(buf.slice_mut(..), file_pos)
                                .map_err(StoreError::IO)?;

                            buf.increase_written(buf.capacity);

                            self.block_cache.add_entry(Arc::new(buf));
                        }
                        Ok(())
                    });

                    // leak the fd from file to avoid closing it.
                    let _ = file.into_raw_fd();
                    try_r?
                }
            }
        }

        // Assert that the length of the given buffer is equal to the sum of the length of the slices.
        debug_assert_eq!(buf.len(), slice_v.iter().map(|s| s.len()).sum::<usize>());

        // Copy the slice_v to the given buffer.
        buf.copy_from_slice(&slice_v.concat()[..]);

        Ok(())
    }

    /// Truncate the segment to the given `written` offset.
    ///
    /// Currently, there are two tasks to do:
    /// * Truncate the block cache to the given `written` offset, i.e. remove the cached blocks after the given `written` offset.
    /// * Split the last page and make it independent, the last page will be updated in the subsequent write operations.
    pub(crate) fn truncate_to(&mut self, written: u64) -> Result<(), StoreError> {
        // Assert the given `written` offset is equal to the current `written` offset.
        debug_assert_eq!(written, self.written);

        // TODO: Consider use a unified way to provide the IO options.
        let alignment = self.config.store.alignment;
        let mut last_page_start = written / alignment as u64 * alignment as u64;
        let last_page_len = (written - last_page_start) as u32;
        last_page_start += self.wal_offset;

        // Retrieve the last page from the block cache.
        let from_wal_offset = if self.wal_offset + written > 0 {
            self.wal_offset + written - 1
        } else {
            0
        };
        let buf_res =
            self.block_cache
                .try_get_entries(EntryRange::new(from_wal_offset, 1, alignment));

        match buf_res {
            Ok(Some(buf_v)) => {
                // Assert there is only one buf returned of the last page.
                debug_assert_eq!(buf_v.len(), 1);
                let buf = &buf_v[0];

                debug_assert!((buf.wal_offset + buf.limit() as u64) >= written);

                // Split the last cache entry from `last_page_start`
                if buf.wal_offset + buf.limit() as u64 > last_page_start {
                    // Split the last page from the returned buf
                    if last_page_len != 0 {
                        let last_page_buf =
                            AlignedBuf::new(last_page_start, last_page_len as usize, alignment)?;

                        let copy_start = (last_page_start - buf.wal_offset) as usize;
                        let buf_src = buf.slice(copy_start..copy_start + last_page_len as usize);

                        last_page_buf
                            .slice_mut(..last_page_len as usize)
                            .copy_from_slice(&buf_src);
                        last_page_buf.increase_written(last_page_len as usize);

                        self.block_cache.add_entry(Arc::new(last_page_buf));
                    }

                    // Truncate the last buf
                    let last_len = last_page_start - buf.wal_offset;
                    if last_len != 0 {
                        let last_buf =
                            AlignedBuf::new(buf.wal_offset, last_len as usize, alignment)?;

                        last_buf
                            .slice_mut(..last_len as usize)
                            .copy_from_slice(&buf.slice(..last_len as usize));

                        last_buf.increase_written(last_len as usize);

                        self.block_cache.add_entry(Arc::new(last_buf));
                    }
                }

                // Evict the cached blocks after the given `written` offset.
                self.block_cache
                    .remove(|buf| buf.wal_offset() >= self.wal_offset + written);
                Ok(())
            }
            Ok(None) => {
                // Impossible, the block cache should always return a result since this is in recovery procedure.
                Err(StoreError::CacheError)
            }
            Err(_) => {
                // The last page is not cached, it's also impossible.
                Err(StoreError::CacheError)
            }
        }
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
        self.wal_offset.partial_cmp(&other.wal_offset)
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
    use crate::io::{
        block_cache::EntryRange,
        buf::{AlignedBuf, AlignedBufWriter},
        record::RecordType,
    };
    use bytes::{BufMut, BytesMut};
    use rand::RngCore;
    use std::{
        error::Error,
        fs::File,
        os::{fd::FromRawFd, unix::prelude::FileExt},
        path::{self, Path},
        sync::Arc,
    };
    use uuid::Uuid;

    use super::{LogSegment, Status};

    #[test]
    fn test_format_number() {
        assert_eq!(
            LogSegment::format(0).len(),
            LogSegment::format(std::u64::MAX).len()
        );
    }

    #[test]
    fn test_append_record() -> Result<(), Box<dyn Error>> {
        let wal_path = test_util::create_random_path()?;
        let _guard = test_util::DirectoryRemovalGuard::new(wal_path.as_path());
        let mut cfg = config::Configuration::default();
        cfg.store.path.set_wal(wal_path.as_path().to_str().unwrap());
        let config = Arc::new(cfg);

        let mut segment = super::LogSegment::new(&config, 0, 1024 * 1024, wal_path.as_path())?;
        segment.status = Status::ReadWrite;

        let mut buf_writer = AlignedBufWriter::new(0, 512);
        buf_writer.reserve_to(1024, config.store.segment_size as usize)?;

        let mut data = BytesMut::with_capacity(256);
        data.resize(256, 65);

        let pos = segment.append_record(&mut buf_writer, &data)?;
        assert_eq!(pos, 4 + 4 + data.len() as u64);

        let buffers = buf_writer.take();
        let buf = buffers.first().unwrap();
        let crc = buf.read_u32(0)?;
        let ckm = util::crc32::crc32(&data);
        let mut total_ckm = bytes::BytesMut::with_capacity(4 + 8);
        total_ckm.put_u32(ckm);
        total_ckm.put_u64(segment.wal_offset);
        let ckm = util::crc32::crc32(total_ckm);
        assert_eq!(crc, ckm);
        let length_type = buf.read_u32(4)?;
        let (len, t) = RecordType::parse(length_type)?;
        assert_eq!(t, RecordType::Full);
        assert_eq!(len as usize, data.len());

        let payload = buf.slice((crate::RECORD_PREFIX_LENGTH as usize)..);
        assert_eq!(data, payload);
        Ok(())
    }

    #[test]
    fn test_read_exact_at() -> Result<(), Box<dyn Error>> {
        let cfg = config::Configuration::default();
        let config = Arc::new(cfg);
        let alignment = config.store.alignment;

        // Start offset of current segment
        let wal_offset = 1024 * 1024;

        let uuid = Uuid::new_v4();

        // Generate some random data
        let buf_w = AlignedBuf::new(wal_offset, 4 * alignment as usize, alignment).unwrap();

        rand::thread_rng().fill_bytes(buf_w.slice_mut(..));
        buf_w.increase_written(buf_w.capacity);

        let mut store_dir = test_util::create_random_path().unwrap();
        let store_dir_c = store_dir.clone();
        let _store_dir_guard =
            test_util::DirectoryRemovalGuard::new(&Path::new(store_dir_c.as_os_str()));

        store_dir.push(path::PathBuf::from(uuid.simple().to_string()));

        let mut segment =
            super::LogSegment::new(&config, wal_offset, 1024 * 1024, store_dir.as_path()).unwrap();
        segment.open().unwrap();

        let sd = segment.sd.as_ref().unwrap();
        // Open the file with the given `fd`.
        let file = unsafe { File::from_raw_fd(sd.fd) };
        // Write pages to file
        file.write_all_at(&buf_w.slice(..), 0).unwrap();

        // Read some bytes in the second page, and test whether the whole second page is cached.
        let mut buf = [0u8; 5];
        segment
            .read_exact_at(&mut buf, alignment as u64 + 2)
            .unwrap();
        assert_eq!(
            &buf,
            &buf_w.slice((alignment + 2) as usize..(alignment + 2 + 5) as usize)[..]
        );

        let buf_v = segment
            .block_cache
            .try_get_entries(EntryRange::new(
                segment.wal_offset + alignment as u64,
                alignment as u32,
                alignment,
            ))
            .unwrap()
            .unwrap();
        assert_eq!(buf_v.len(), 1);
        let buf = buf_v.first().unwrap();
        assert_eq!(buf.wal_offset, segment.wal_offset + alignment as u64);
        assert_eq!(buf.limit(), alignment as usize);
        assert_eq!(
            buf.slice(..),
            buf_w.slice(alignment as usize..(alignment + alignment) as usize)
        );

        // Read some bytes cross the page boundary, and test whether the whole two pages is cached.
        let mut buf = [0u8; 5];
        segment
            .read_exact_at(&mut buf, alignment as u64 * 3 - 2)
            .unwrap();
        assert_eq!(
            &buf,
            &buf_w.slice((alignment * 3 - 2) as usize..(alignment * 3 - 2 + 5) as usize)[..]
        );

        let buf_v = segment
            .block_cache
            .try_get_entries(EntryRange::new(
                segment.wal_offset + alignment as u64 * 2,
                alignment as u32 * 2,
                alignment,
            ))
            .unwrap()
            .unwrap();
        assert_eq!(buf_v.len(), 1);
        let buf = buf_v.first().unwrap();
        assert_eq!(
            buf.slice(..),
            buf_w.slice((alignment * 2) as usize..(alignment * 4) as usize)
        );
        Ok(())
    }

    #[test]
    fn test_truncate_to() -> Result<(), Box<dyn Error>> {
        let mut cfg = config::Configuration::default();
        cfg.store.alignment = 4096;
        let config = Arc::new(cfg);
        let alignment = config.store.alignment;

        // Start offset of current segment
        let wal_offset = 1024 * 1024;

        let uuid = Uuid::new_v4();

        // Generate some random data
        let buf_w = AlignedBuf::new(wal_offset, 4 * alignment, alignment).unwrap();

        rand::thread_rng().fill_bytes(buf_w.slice_mut(..));
        buf_w.increase_written(buf_w.capacity);

        let mut store_dir = test_util::create_random_path().unwrap();
        let store_dir_c = store_dir.clone();
        let _store_dir_guard =
            test_util::DirectoryRemovalGuard::new(&Path::new(store_dir_c.as_os_str()));

        store_dir.push(path::PathBuf::from(uuid.simple().to_string()));

        let mut segment =
            super::LogSegment::new(&config, wal_offset, 1024 * 1024, store_dir.as_path()).unwrap();
        segment.open().unwrap();

        let sd = segment.sd.as_ref().unwrap();
        // Open the file with the given `fd`.
        let file = unsafe { File::from_raw_fd(sd.fd) };
        // Write pages to file
        file.write_all_at(&buf_w.slice(..), 0).unwrap();

        {
            // Case one: the last page is already cached individually.
            segment.written = 4 * alignment as u64;
            // Load the first three pages.
            let mut buf = [0u8; 3 * 4096 as usize];
            segment.read_exact_at(&mut buf, 0).unwrap();
            // Load the last page
            let mut buf = [0u8; 4096 as usize];
            segment
                .read_exact_at(&mut buf, 3 * alignment as u64)
                .unwrap();

            segment.truncate_to(4 * alignment as u64).unwrap();

            // Assert nothing has been changed for block cache.
            let buf_v = segment
                .block_cache
                .try_get_entries(EntryRange::new(wal_offset, alignment as u32, alignment))
                .unwrap()
                .unwrap();
            assert_eq!(buf_v.len(), 1);
            let buf = buf_v.first().unwrap();
            assert_eq!(buf.wal_offset, wal_offset);
            assert_eq!(buf.limit(), alignment as usize * 3);

            let buf_v = segment
                .block_cache
                .try_get_entries(EntryRange::new(
                    wal_offset + 3 * alignment as u64,
                    alignment as u32,
                    alignment,
                ))
                .unwrap()
                .unwrap();
            assert_eq!(buf_v.len(), 1);
            let buf = buf_v.first().unwrap();
            assert_eq!(buf.limit(), alignment as usize);
            assert_eq!(buf.capacity, alignment as usize);

            // Clear the cache
            segment.block_cache.remove(|t| t.wal_offset() >= wal_offset);
        }

        {
            // Case two: the last page is already cached individually, but some dirty cache is loaded.
            // Load the first three pages.
            let mut buf = [0u8; 3 * 4096 as usize];
            segment.read_exact_at(&mut buf, 0).unwrap();
            // Load the last page and a dirty page
            let mut buf = [0u8; 4096 * 2 as usize];
            segment
                .read_exact_at(&mut buf, 3 * alignment as u64)
                .unwrap();

            segment.written = 4 * alignment as u64 - 1024;
            segment.truncate_to(4 * alignment as u64 - 1024).unwrap();

            let buf_v = segment
                .block_cache
                .try_get_entries(EntryRange::new(wal_offset, alignment as u32, alignment))
                .unwrap()
                .unwrap();
            assert_eq!(buf_v.len(), 1);
            let buf = buf_v.first().unwrap();
            assert_eq!(buf.wal_offset, wal_offset);
            assert_eq!(buf.limit(), alignment as usize * 3);

            let buf_v = segment
                .block_cache
                .try_get_entries(EntryRange::new(
                    wal_offset + 3 * alignment as u64,
                    1 as u32,
                    alignment,
                ))
                .unwrap()
                .unwrap();
            assert_eq!(buf_v.len(), 1);
            let buf = buf_v.first().unwrap();
            assert_eq!(buf.wal_offset, wal_offset + 3 * alignment as u64);
            assert_eq!(buf.limit(), alignment as usize - 1024);
            assert_eq!(buf.capacity, alignment as usize);

            // Clear the cache
            segment.block_cache.remove(|t| t.wal_offset() >= wal_offset);
        }

        {
            // Case three: the last page should be splitted, with dirty cache.
            // Load the all the four pages.
            let mut buf = [0u8; 4 * 4096 as usize];
            segment.read_exact_at(&mut buf, 0).unwrap();

            segment.written = 3 * alignment as u64 + 1024;
            segment.truncate_to(3 * alignment as u64 + 1024).unwrap();

            let buf_v = segment
                .block_cache
                .try_get_entries(EntryRange::new(wal_offset, alignment as u32, alignment))
                .unwrap()
                .unwrap();
            assert_eq!(buf_v.len(), 1);
            let buf = buf_v.first().unwrap();
            assert_eq!(buf.wal_offset, wal_offset);
            assert_eq!(buf.limit(), alignment as usize * 3);

            let buf_v = segment
                .block_cache
                .try_get_entries(EntryRange::new(
                    wal_offset + 3 * alignment as u64,
                    1024 as u32,
                    alignment,
                ))
                .unwrap()
                .unwrap();
            assert_eq!(buf_v.len(), 1);
            let buf = buf_v.first().unwrap();
            assert_eq!(buf.limit(), 1024 as usize);
            assert_eq!(buf.capacity, alignment as usize);

            // Clear the cache
            segment.block_cache.remove(|t| t.wal_offset() >= wal_offset);
        }

        Ok(())
    }
}
