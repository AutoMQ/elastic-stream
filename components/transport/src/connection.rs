use bytes::{Buf, BytesMut};
use local_sync::{mpsc, oneshot};
use log::{error, info, trace, warn};
use std::cell::UnsafeCell;
use std::io::Cursor;
use std::rc::Rc;
use tokio_uring::net::TcpStream;

use codec::error::FrameError;
use codec::frame::Frame;

use crate::WriteTask;

const BUFFER_SIZE: usize = 4 * 1024;

pub struct Connection {
    /// Underlying TCP stream.
    stream: Rc<TcpStream>,

    /// Read buffer for this connection.
    buffer: UnsafeCell<BytesMut>,

    peer_address: String,

    /// Write buffer for this connection.
    ///
    /// Writes from concurrent coroutines are serialized by this MPSC channel.
    tx: mpsc::unbounded::Tx<WriteTask>,
}

impl Connection {
    pub fn new(stream: TcpStream, peer_address: &str) -> Self {
        let stream = Rc::new(stream);
        let (tx, mut rx) = mpsc::unbounded::channel::<WriteTask>();

        let write_stream = Rc::clone(&stream);
        let target_address = peer_address.to_owned();
        tokio_uring::spawn(async move {
            log::trace!("Start write coroutine loop for {}", &target_address);
            loop {
                match rx.recv().await {
                    Some(task) => {
                        log::trace!(
                            "Write-frame-task[stream-id={}] received, start writing to {}",
                            task.frame.stream_id,
                            &target_address
                        );
                        Self::write(&write_stream, task, &target_address).await;
                    }
                    None => {
                        log::info!(
                            "Connection to {} should be closed, stop write coroutine loop",
                            &target_address
                        );
                        break;
                    }
                }
            }
        });

        Self {
            stream,
            buffer: UnsafeCell::new(BytesMut::with_capacity(BUFFER_SIZE)),
            peer_address: peer_address.to_owned(),
            tx,
        }
    }

    async fn write(stream: &Rc<TcpStream>, task: WriteTask, peer_address: &str) {
        let mut buffers = match task.frame.encode().map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to encode frame. Cause: {:?}", e),
            )
        }) {
            Ok(buffers) => buffers,
            Err(e) => {
                error!("Failed to encode Frame[stream-id={}]", task.frame.stream_id);
                let _ = task.observer.send(Err(e));
                return;
            }
        };

        let total = buffers.iter().map(|b| b.len()).sum::<usize>();
        trace!("Get {} bytes to write to: {}", total, peer_address);
        let mut remaining = total;
        loop {
            let (res, _buffers) = stream.writev(buffers).await;
            buffers = _buffers;
            match res {
                Ok(mut n) => {
                    debug_assert!(n <= remaining, "Data written to socket buffer should be less than or equal to remaining bytes to write");
                    if n == remaining {
                        if remaining == total {
                            // First time to write
                            trace!("Wrote {}/{} bytes to {}", n, total, peer_address);
                        } else {
                            // Last time of writing: the remaining are all written.
                            remaining -= n;
                            trace!(
                                "Wrote {}/{} bytes to {}. Overall, {}/{} is written.",
                                n,
                                total,
                                peer_address,
                                total - remaining,
                                total
                            );
                        }
                        break;
                    } else {
                        remaining -= n;
                        trace!(
                            "Wrote {} bytes to {}. Overall, {}/{} is written",
                            n,
                            peer_address,
                            total - remaining,
                            total,
                        );
                        // Drain/advance buffers that are already written.
                        buffers
                            .extract_if(|buffer| {
                                if buffer.len() <= n {
                                    n -= buffer.len();
                                    // Remove it
                                    true
                                } else {
                                    if n > 0 {
                                        buffer.advance(n);
                                        n = 0;
                                    }
                                    // Keep the buffer slice
                                    false
                                }
                            })
                            .count();
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to write Frame[stream-id={}] to {peer_address}",
                        task.frame.stream_id
                    );
                    let _ = task.observer.send(Err(e));
                    return;
                }
            };
        }

        let _ = task.observer.send(Ok(()));
    }

    pub fn peer_address(&self) -> &str {
        &self.peer_address
    }

    #[allow(clippy::mut_from_ref)]
    pub fn buf_mut(&self) -> &mut BytesMut {
        unsafe { &mut *self.buffer.get() }
    }

    pub fn buf(&self) -> &BytesMut {
        unsafe { &*self.buffer.get() }
    }

    /// Read a single `Frame` value from the underlying stream.
    ///
    /// The function waits until it has retrieved enough data to parse a frame.
    /// Any data remaining in the read buffer after the frame has been parsed is
    /// kept there for the next call to `read_frame`.
    ///
    /// # Returns
    ///
    /// On success, the received frame is returned. If the `TcpStream`
    /// is closed in a way that doesn't break a frame in half, it returns
    /// `None`. Otherwise, an error is returned.
    pub async fn read_frame(&self) -> Result<Option<Frame>, FrameError> {
        loop {
            // Attempt to parse a frame from the buffered data. If enough data
            // has been buffered, the frame is returned.
            if let Some(frame) = self.parse_frame()? {
                return Ok(Some(frame));
            }

            // Try to allocate more memory from allocator
            let buffer = self.buf_mut();
            if buffer.spare_capacity_mut().len() < BUFFER_SIZE {
                buffer.reserve(BUFFER_SIZE);
            }

            // There is not enough buffered data to read a frame. Attempt to
            // read more data from the socket.
            //
            // On success, the number of bytes is returned. `0` indicates "end
            // of stream".
            let len = buffer.len();
            let buf = buffer.split_off(len);
            let (res, buf) = self.stream.read(buf).await;
            buffer.unsplit(buf);

            let read = match res {
                Ok(n) => {
                    trace!("Read {} bytes from {}", n, self.peer_address);
                    n
                }
                Err(_e) => {
                    info!("Failed to read data from {}", self.peer_address);
                    0
                }
            };

            if 0 == read {
                // The remote closed the connection. For this to be a clean
                // shutdown, there should be no data in the read buffer. If
                // there is, this means that the peer closed the socket while
                // sending a frame.
                if buffer.is_empty() {
                    return Ok(None);
                } else {
                    warn!("Discarded {} bytes", buffer.len());
                    return Err(FrameError::ConnectionReset);
                }
            }
        }
    }

    /// Tries to parse a frame from the buffer. If the buffer contains enough
    /// data, the frame is returned and the data removed from the buffer. If not
    /// enough data has been buffered yet, `Ok(None)` is returned. If the
    /// buffered data does not represent a valid frame, `Err` is returned.
    fn parse_frame(&self) -> Result<Option<Frame>, FrameError> {
        use FrameError::Incomplete;

        // Cursor is used to track the "current" location in the
        // buffer. Cursor also implements `Buf` from the `bytes` crate
        // which provides a number of helpful utilities for working
        // with bytes.
        let buffer = self.buf();
        let mut buf = Cursor::new(&buffer[..]);

        // The first step is to check if enough data has been buffered to parse
        // a single frame. This step is usually much faster than doing a full
        // parse of the frame, and allows us to skip allocating data structures
        // to hold the frame data unless we know the full frame has been
        // received.
        match Frame::check(&mut buf) {
            Ok(_) => {
                // The `check` function will have advanced the cursor until the
                // end of the frame. Since the cursor had position set to zero
                // before `Frame::check` was called, we obtain the length of the
                // frame by checking the cursor position.
                let len = buf.position() as usize;

                // Reset the position to zero before passing the cursor to
                // `Frame::parse`.
                buf.set_position(0);

                // Parse the frame from the buffer. This allocates the necessary
                // structures to represent the frame and returns the frame
                // value.
                //
                // If the encoded frame representation is invalid, an error is
                // returned. This should terminate the **current** connection
                // but should not impact any other connected client.
                let frame = Frame::parse(&mut buf)?;

                // Discard the parsed data from the read buffer.
                //
                // When `advance` is called on the read buffer, all of the data
                // up to `len` is discarded. The details of how this works is
                // left to `BytesMut`. This is often done by moving an internal
                // cursor, but it may be done by reallocating and copying data.
                self.buf_mut().advance(len);

                // Return the parsed frame to the caller.
                Ok(Some(frame))
            }

            // There is not enough data present in the read buffer to parse a
            // single frame. We must wait for more data to be received from the
            // socket. Reading from the socket will be done in the statement
            // after this `match`.
            //
            // We do not want to return `Err` from here as this "error" is an
            // expected runtime condition.
            Err(Incomplete) => Ok(None),

            // An error was encountered while parsing the frame. The connection
            // is now in an invalid state. Returning `Err` from here will result
            // in the connection being closed.
            Err(e) => Err(e),
        }
    }

    pub async fn write_frame(&self, frame: Frame) -> Result<(), std::io::Error> {
        let (tx, rx) = oneshot::channel();
        let task = WriteTask {
            frame,
            observer: tx,
        };
        self.tx.send(task).map_err(|e| {
            warn!(
                "Failed to send frame to connection's internal SPSC channel. Cause: {:?}",
                e
            );
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Connection's internal SPSC channel is closed. Cause: {:?}",
                    e
                ),
            )
        })?;

        rx.await.map_err(|e| {
            warn!(
                "Failed to receive acknowledgement from oneshot channel. Cause: {:?}",
                e
            );
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Connection's internal oneshot channel is closed. Cause: {:?}",
                    e
                ),
            )
        })?
    }

    pub fn close(&self) -> std::io::Result<()> {
        self.stream.shutdown(std::net::Shutdown::Both)
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, thread::JoinHandle};

    use bytes::{Buf, BufMut, BytesMut};
    use protocol::rpc::header::OperationCode;
    use tokio::{io::AsyncReadExt, net::TcpListener};

    #[test]
    fn test_bytes_concept() {
        let mut buf = BytesMut::with_capacity(128);
        buf.put_i32(1);
        assert_eq!(128, buf.capacity());
        assert_eq!(4, buf.len());
        assert_eq!(4, buf.remaining());
        assert_eq!(124, buf.spare_capacity_mut().len());

        buf.get_u16();

        assert_eq!(126, buf.capacity());
        assert_eq!(2, buf.len());
        assert_eq!(2, buf.remaining());
        assert_eq!(124, buf.spare_capacity_mut().len());

        buf.put_u8(4u8);
        assert_eq!(126, buf.capacity());
        assert_eq!(3, buf.len());
        assert_eq!(3, buf.remaining());
        assert_eq!(123, buf.spare_capacity_mut().len());

        buf.put_u8(2u8);
        assert_eq!(126, buf.capacity());
        assert_eq!(4, buf.len());
        assert_eq!(4, buf.remaining());
        assert_eq!(122, buf.spare_capacity_mut().len());

        buf.get_u32();
        assert_eq!(122, buf.capacity());
        assert_eq!(0, buf.len());
        assert_eq!(0, buf.remaining());
        assert_eq!(122, buf.spare_capacity_mut().len());
    }

    fn run_server(
        counter: tokio::sync::oneshot::Sender<usize>,
        port: tokio::sync::oneshot::Sender<u16>,
    ) -> Result<JoinHandle<()>, Box<dyn Error>> {
        let handle = std::thread::Builder::new()
            .name("Test-Server".to_owned())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(async move {
                    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let local_addr = listener.local_addr().unwrap();
                    port.send(local_addr.port()).unwrap();
                    let (mut stream, _addr) = listener.accept().await.unwrap();
                    let mut read = 0;
                    let mut buf = BytesMut::with_capacity(1024);
                    while let Ok(n) = stream.read_buf(&mut buf).await {
                        if 0 == n {
                            break;
                        }
                        read += n;
                        buf.clear();
                    }
                    counter.send(read).unwrap();
                });
            })?;
        Ok(handle)
    }

    #[test]
    fn test_write_frame() -> Result<(), Box<dyn Error>> {
        crate::log::try_init_log();
        let (counter_tx, counter_rx) = tokio::sync::oneshot::channel();
        let (port_tx, port_rx) = tokio::sync::oneshot::channel();

        let handle = run_server(counter_tx, port_tx).unwrap();

        let port = port_rx.blocking_recv()?;

        tokio_uring::start(async {
            let address = format!("127.0.0.1:{}", port);
            {
                let tcp_stream = tokio_uring::net::TcpStream::connect(address.parse().unwrap())
                    .await
                    .unwrap();
                tcp_stream.set_nodelay(true).unwrap();
                let connection = super::Connection::new(tcp_stream, &address);
                let mut frame = codec::frame::Frame::new(OperationCode::ALLOCATE_ID);
                let mut payload = vec![];
                (0..8).for_each(|_| {
                    let mut buf = BytesMut::with_capacity(1024 * 1024);
                    buf.resize(1024 * 1024, 8u8);
                    payload.push(buf.freeze());
                });
                frame.payload = Some(payload);
                connection.write_frame(frame).await.unwrap();
                connection.close().unwrap();
            }

            let total = counter_rx.await.unwrap();
            assert_eq!(total, 1024 * 1024 * 8 + 20);
        });

        let _ = handle.join();

        Ok(())
    }
}
