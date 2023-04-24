use std::{ops::Deref, sync::Arc};

use tokio_uring::buf::IoBuf;

use super::AlignedBuf;

/// Expose cached `Record` to application layer.
///
/// This struct is designed to be pub and `Send`.
#[derive(Debug, Clone)]
pub struct BufSlice {
    /// Shared block cache.
    buf: Arc<AlignedBuf>,

    /// Position within the aligned buffer where the expected payload starts.
    pos: u32,

    /// Limit boundary of the requested `Record`.
    limit: u32,
}

impl BufSlice {
    /// Constructor of the `BufSlice`. Expected to be used internally in the store crate.
    pub(crate) fn new(buf: Arc<AlignedBuf>, pos: u32, limit: u32) -> Self {
        debug_assert!(pos <= limit);
        debug_assert!(limit <= buf.capacity as u32);
        Self { buf, pos, limit }
    }
}

/// Implement `Deref` to `&[u8]` so that application developers may inspect this buffer conveniently.
impl Deref for BufSlice {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        let base = unsafe { self.buf.as_ptr().offset(self.pos as isize) };
        unsafe { std::slice::from_raw_parts(base, (self.limit - self.pos) as usize) }
    }
}

/// Implement `Into<&[u8]>`, such that data can be transferred to network without copy.
///
/// If encryption is not needed, we are offering zero-copy out of box.
impl From<&BufSlice> for &[u8] {
    fn from(value: &BufSlice) -> Self {
        let base = unsafe { value.buf.as_ptr().offset(value.pos as isize) };
        unsafe { std::slice::from_raw_parts(base, (value.limit - value.pos) as usize) }
    }
}

/// Implement IoBuf from tokio_uring so that direct write is possible.
unsafe impl IoBuf for BufSlice {
    fn stable_ptr(&self) -> *const u8 {
        unsafe { self.buf.as_ptr().offset(self.pos as isize) }
    }

    fn bytes_init(&self) -> usize {
        (self.limit - self.pos) as usize
    }

    fn bytes_total(&self) -> usize {
        (self.limit - self.pos) as usize
    }
}

/// Safety: Since `BufSlice` holds an atomic reference to append-only `AlignedBuf` and
/// `BufSlice` itself is read-only. It's safe to send it across threads.
///
/// See https://doc.rust-lang.org/nomicon/send-and-sync.html
unsafe impl Send for BufSlice {}

/// Safety: `BufSlice` is read-only and therefore sync.
///
/// See https://doc.rust-lang.org/nomicon/send-and-sync.html
unsafe impl Sync for BufSlice {}

#[cfg(test)]
mod tests {
    use crate::io::buf::AlignedBuf;
    use log::{info, trace};
    use std::{error::Error, sync::Arc};
    use tokio::sync::oneshot;
    use tokio_uring::net::{TcpListener, TcpStream};

    #[test]
    fn test_buf_slice() -> Result<(), Box<dyn Error>> {
        let aligned_buf = Arc::new(AlignedBuf::new(0, 4096, 4096)?);
        let data = "Hello";
        aligned_buf.write_u32(0, data.len() as u32);
        aligned_buf.write_buf(4, data.as_bytes());
        let slice_buf = super::BufSlice::new(Arc::clone(&aligned_buf), 4, 4 + data.len() as u32);

        // Verify `Deref` trait
        assert_eq!(data.len(), slice_buf.len());

        // Verify `From` trait
        let greeting = std::str::from_utf8((&slice_buf).into())?;
        assert_eq!(data, greeting);

        let cloned = slice_buf.clone();
        let ptr = &cloned;

        std::thread::scope(|scope| {
            scope.spawn(move || {
                let _ = slice_buf.len();
                let _ = ptr.len();
                info!("Scoped thread {:?} completed", std::thread::current().id());
            });
        });

        let _ = cloned.len();
        info!("main thread {:?} completed", std::thread::current().id());

        Ok(())
    }

    /// Add usage test for network.
    #[test]
    fn test_network() -> Result<(), Box<dyn Error>> {
        tokio_uring::start(async {
            let (tx, rx) = oneshot::channel();
            tokio_uring::spawn(async move {
                let listener = TcpListener::bind("[::]:0".parse().unwrap()).unwrap();
                let port = listener.local_addr().unwrap().port();
                tx.send(port).unwrap();
                let (stream, _addr) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 4096];
                loop {
                    let (result, nbuf) = stream.read(buf).await;
                    buf = nbuf;
                    let read = result.unwrap();
                    if read == 0 {
                        break;
                    }
                    let s = unsafe { std::slice::from_raw_parts(buf.as_ptr(), read) };
                    let read_str = std::str::from_utf8(s).unwrap();
                    trace!("{}", read_str);
                }
            });

            let port = rx.await?;

            let aligned_buf = Arc::new(AlignedBuf::new(0, 4096, 4096)?);
            let data = "Hello";
            aligned_buf.write_u32(0, data.len() as u32);
            aligned_buf.write_buf(4, data.as_bytes());

            let buffers: Vec<_> = (0..16)
                .map(|_n| super::BufSlice::new(Arc::clone(&aligned_buf), 4, 4 + data.len() as u32))
                .collect();

            let stream = TcpStream::connect(format!("127.0.0.1:{}", port).parse()?).await?;

            let _res = stream.writev(buffers).await;
            Ok(())
        })
    }
}
