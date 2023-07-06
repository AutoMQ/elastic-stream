use core::slice::SlicePattern;
use std::io::IoSlice;

use bytes::Buf;

pub struct BytesSliceCursor<'a> {
    bufs: &'a mut [IoSlice<'a>],
    remaining: usize,
}

impl<'a> BytesSliceCursor<'a> {
    pub fn new(bufs: &'a mut [IoSlice<'a>]) -> Self {
        let remaining = bufs.iter().map(|buf| buf.len()).sum();
        Self { bufs, remaining }
    }
}

impl<'a> Buf for BytesSliceCursor<'a> {
    fn remaining(&self) -> usize {
        self.remaining
    }

    fn chunk(&self) -> &[u8] {
        match self.bufs.first() {
            None => &[],
            Some(buf) => buf.as_slice(),
        }
    }

    fn advance(&mut self, cnt: usize) {
        self.remaining -= cnt;
        IoSlice::advance_slices(&mut self.bufs, cnt);
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;

    use super::*;

    #[test]
    fn test_slices() {
        let buf1 = Bytes::copy_from_slice(b"abc");
        let buf2 = Bytes::copy_from_slice(b"def");
        let buffers = vec![buf1, buf2];
        let mut slices = buffers
            .iter()
            .map(|buf| IoSlice::new(&buf[..]))
            .collect::<Vec<_>>();
        let mut cursor = super::BytesSliceCursor::new(&mut slices);
        assert_eq!(6, cursor.remaining());
        cursor.advance(2);
        assert_eq!(4, cursor.remaining());
        assert_eq!(cursor.get_u8(), b'c');
        assert_eq!(cursor.get_u8(), b'd');
    }
}
