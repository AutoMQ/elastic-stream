mod aligned_buf;
mod aligned_buf_reader;
mod aligned_buf_writer;

pub(crate) use self::aligned_buf::AlignedBuf;
pub(crate) use self::aligned_buf_reader::AlignedBufReader;
pub(crate) use self::aligned_buf_writer::AlignedBufWriter;

#[cfg(test)]
mod tests {
    use std::{alloc::Layout, error::Error, ffi::CStr};

    use bytes::{BufMut, BytesMut};

    /// Play with Rust ptr.
    #[test]
    fn test_layout() -> Result<(), Box<dyn Error>> {
        let alignment = 4096;
        let len = 512;
        let capacity = (len + alignment - 1) / alignment * alignment;
        let layout = Layout::from_size_align(capacity, alignment)?;

        assert_eq!(alignment, layout.align());
        assert_eq!(alignment, capacity);

        let ptr = unsafe { std::alloc::alloc(layout) };
        let mut buf = BytesMut::with_capacity(alignment);
        buf.resize(alignment - 1, 65);
        buf.put_i8(0);
        unsafe { std::ptr::copy_nonoverlapping(buf.as_ptr(), ptr, alignment) };
        let s = unsafe { CStr::from_ptr(ptr as *const libc::c_char) };
        assert_eq!(alignment - 1, s.to_str()?.len());
        unsafe { std::alloc::dealloc(ptr, layout) };
        Ok(())
    }
}
