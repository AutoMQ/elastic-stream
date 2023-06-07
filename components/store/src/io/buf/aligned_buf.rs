use log::debug;
use std::{
    alloc::{self, Layout},
    fmt::{self, Display, Formatter},
    ops::{Bound, RangeBounds},
    ptr::{self, NonNull},
    slice,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::error::StoreError;

/// Memory buffer complying given memory alignment, which is supposed to be used for DirectIO.
///
/// This struct is designed to be NOT `Copy` nor `Clone`; otherwise, we will have double-free issue.
#[derive(Debug)]
pub(crate) struct AlignedBuf {
    /// An aligned WAL offset which is an absolute address in the WAL.
    pub(crate) wal_offset: u64,

    /// Pointer to the allocated memory
    ptr: NonNull<u8>,

    layout: Layout,

    pub(crate) capacity: usize,

    /// Write index
    ///
    /// Data within [0, limit) are valid whilst [limit, capacity) are uninitialized.
    ///
    /// # Warning
    /// We need keep limit atomic since reference of it may be shared to other threads to read the valid
    /// parts and `BufWriter` expands the valid boundary concurrently.
    pub(crate) limit: AtomicUsize,
}

impl AlignedBuf {
    pub(crate) fn new(wal_offset: u64, len: usize, alignment: usize) -> Result<Self, StoreError> {
        debug_assert!(len > 0, "Memory to allocate should be positive");
        debug_assert!(alignment > 0, "Alignment should be positive");
        debug_assert!(
            alignment.is_power_of_two(),
            "Alignment should be power of 2"
        );

        let capacity = (len + alignment - 1) / alignment * alignment;
        let layout = Layout::from_size_align(capacity, alignment)
            .map_err(|_e| StoreError::MemoryAlignment)?;

        // Safety
        // alloc may return null if memory is exhausted or layout does not meet allocator's size or alignment constraint.
        let ptr = unsafe { alloc::alloc_zeroed(layout) };

        let ptr = match NonNull::<u8>::new(ptr) {
            Some(ptr) => ptr,
            None => {
                // Crash eagerly to facilitate root-cause-analysis.
                return Err(StoreError::OutOfMemory);
            }
        };

        Ok(Self {
            wal_offset,
            ptr,
            layout,
            capacity,
            limit: AtomicUsize::new(0),
        })
    }

    /// Judge if this buffer covers specified data region in WAL.
    ///
    /// # Arguments
    /// * `wal_offset` - Offset in WAL
    /// * `len` - Length of the data.
    ///
    /// # Returns
    /// `true` if the cache hit; `false` otherwise.
    #[allow(dead_code)]
    pub(crate) fn covers(&self, wal_offset: u64, len: u32) -> bool {
        self.wal_offset <= wal_offset
            && wal_offset + len as u64 <= self.wal_offset + self.limit() as u64
    }

    /// Judge if this buffer covers specified data region partially in WAL.
    ///
    /// # Arguments
    /// * `wal_offset` - Offset in WAL
    /// * `len` - Length of the data.
    ///
    /// # Returns
    /// `true` if the cache hit partially;
    /// `false` if the cache has no overlap with the specified region.
    pub(crate) fn covers_partial(&self, wal_offset: u64, len: u32) -> bool {
        self.wal_offset <= wal_offset + len as u64
            && wal_offset <= self.wal_offset + self.limit() as u64
    }

    pub(crate) fn limit(&self) -> usize {
        self.limit.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    pub(crate) fn write_u32(&self, cursor: u64, value: u32) -> bool {
        if self.limit.load(Ordering::Relaxed) + 4 > self.capacity {
            return false;
        }
        let big_endian = value.to_be();
        let data = unsafe { slice::from_raw_parts(ptr::addr_of!(big_endian) as *const u8, 4) };
        self.write_buf(cursor, data)
    }

    /// Get u32 in big-endian byte order.
    #[allow(dead_code)]
    pub(crate) fn read_u32(&self, pos: usize) -> Result<u32, StoreError> {
        debug_assert!(self.limit.load(Ordering::Relaxed) >= pos);
        if self.limit.load(Ordering::Relaxed) - pos < std::mem::size_of::<u32>() {
            return Err(StoreError::InsufficientData);
        }
        let value = unsafe { ptr::read_unaligned::<u32>(self.ptr.as_ptr().add(pos) as *const u32) };
        Ok(u32::from_be(value))
    }

    #[allow(dead_code)]
    pub(crate) fn write_u64(&self, cursor: u64, value: u64) -> bool {
        if self.limit.load(Ordering::Relaxed) + 8 > self.capacity {
            return false;
        }
        let big_endian = value.to_be();
        let data = unsafe { slice::from_raw_parts(ptr::addr_of!(big_endian) as *const u8, 8) };
        self.write_buf(cursor, data)
    }

    #[allow(dead_code)]
    pub(crate) fn read_u64(&self, pos: usize) -> Result<u64, StoreError> {
        debug_assert!(self.limit.load(Ordering::Relaxed) > pos);
        if pos + 8 > self.limit.load(Ordering::Relaxed) {
            return Err(StoreError::InsufficientData);
        }

        let value = unsafe { ptr::read_unaligned::<u64>(self.ptr.as_ptr().add(pos) as *const u64) };
        Ok(u64::from_be(value))
    }

    pub(crate) fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr() as *const u8
    }

    pub(crate) fn slice<R>(&self, range: R) -> &[u8]
    where
        R: RangeBounds<usize>,
    {
        let start = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n.checked_add(1).expect("out of bound"),
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&m) => m.checked_add(1).expect("out of bound"),
            Bound::Excluded(&m) => m,
            Bound::Unbounded => self.limit.load(Ordering::Relaxed),
        };
        let len = end - start;
        unsafe { slice::from_raw_parts(self.ptr.as_ptr().add(start) as *const u8, len) }
    }

    /// Generate a mutable slice from the buffer.
    /// Since it's aimed to be written, the upper bound is equal to the capacity.
    #[allow(clippy::mut_from_ref)]
    pub(crate) fn slice_mut<R>(&self, range: R) -> &mut [u8]
    where
        R: RangeBounds<usize>,
    {
        let start = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n.checked_add(1).expect("out of bound"),
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&m) => m.checked_add(1).expect("out of bound"),
            Bound::Excluded(&m) => m,
            Bound::Unbounded => self.capacity,
        };
        let len = end - start;
        unsafe { slice::from_raw_parts_mut(self.ptr.as_ptr().add(start) as *mut u8, len) }
    }

    pub(crate) fn write_buf(&self, cursor: u64, buf: &[u8]) -> bool {
        let pos = self.limit.load(Ordering::Relaxed);
        debug_assert_eq!(
            cursor,
            self.wal_offset + pos as u64,
            "BufWriter#cursor == AlignedBuf#wal_offset + limit"
        );
        if pos + buf.len() > self.capacity {
            return false;
        }
        unsafe { ptr::copy_nonoverlapping(buf.as_ptr(), self.ptr.as_ptr().add(pos), buf.len()) };
        self.limit.fetch_add(buf.len(), Ordering::Relaxed);
        true
    }

    /// Increase the written position when uring io completion.
    pub(crate) fn increase_written(&self, len: usize) {
        self.limit.fetch_add(len, Ordering::Relaxed);
    }

    /// Remaining space to write.
    pub(crate) fn remaining(&self) -> usize {
        let pos = self.limit.load(Ordering::Relaxed);
        debug_assert!(pos <= self.capacity);
        self.capacity - pos
    }

    pub(crate) fn partial(&self) -> bool {
        self.limit() > 0 && self.limit() < self.capacity
    }

    pub(crate) fn has_data(&self) -> bool {
        self.limit() > 0
    }
}

/// Return the memory back to allocator.
impl Drop for AlignedBuf {
    fn drop(&mut self) {
        unsafe { alloc::dealloc(self.ptr.as_ptr(), self.layout) };
        debug!("Deallocated {}", self);
    }
}

impl Display for AlignedBuf {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`AlignedBuf`={{wal_offset={}, limit={}, capacity={}}}",
            self.wal_offset,
            self.limit(),
            self.capacity
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, mem::size_of};

    use super::AlignedBuf;

    #[test]
    fn test_aligned_buf() -> Result<(), Box<dyn Error>> {
        let alignment = 4096;
        let buf = AlignedBuf::new(0, 128, alignment)?;
        assert_eq!(alignment as usize, buf.remaining());
        let v = 1;
        buf.write_u32(0, 1);
        assert_eq!(buf.remaining(), 4096 - size_of::<u32>());

        let value = buf.read_u32(0)?;
        assert_eq!(v, value);

        let v = 42;
        buf.write_u64(4, v);

        assert_eq!(v, buf.read_u64(4)?);

        let msg = "hello world";
        buf.write_buf(12, msg.as_bytes());
        assert_eq!(buf.remaining(), 4096 - 4 - 8 - msg.as_bytes().len());

        let payload = std::str::from_utf8(buf.slice(12..))?;
        assert_eq!(payload, msg);
        Ok(())
    }

    #[derive(Debug, Clone, Copy)]
    struct Foo {
        i: usize,
    }

    impl Foo {
        fn foo(self) -> usize {
            self.i
        }
    }

    #[test]
    fn test_copy() {
        let f = Foo { i: 1 };
        let x = f.foo();
        let y = f.foo();
        let z = f.foo();

        assert_eq!(1, x);
        assert_eq!(1, y);
        assert_eq!(1, z);
    }
}
