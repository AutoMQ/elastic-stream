use std::io::Cursor;

use bytes::{Buf, BytesMut};
use monoio::{
    io::{AsyncReadRent, BufWriter},
    net::TcpStream,
};

use crate::error::FrameError;
use crate::frame::Frame;

/// Send and receive `Frame` values from a remote peer.
///
/// When implementing networking protocols, a message on that protocol is
/// often composed of several smaller messages known as frames. The purpose of
/// `Connection` is to read and write frames on the underlying `TcpStream`.
///
/// To read frames, the `Connection` uses an internal buffer, which is filled
/// up until there are enough bytes to create a full frame. Once this happens,
/// the `Connection` creates the frame and returns it to the caller.
///
/// When sending frames, the frame is first encoded into the write buffer.
/// The contents of the write buffer are then written to the socket.
pub struct Connection {
    stream: BufWriter<TcpStream>,

    // The buffer for reading frames.
    buffer: BytesMut,
}

impl Connection {
    /// Create a new `Connection`, backed by socket `stream`. Read and write buffers
    /// are initialized.
    pub fn new(stream: TcpStream) -> Self {
        Connection {
            stream: BufWriter::new(stream),
            buffer: BytesMut::with_capacity(4 * 1024),
        }
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
    pub async fn read_frame(&mut self) -> Result<Option<Frame>, FrameError> {
        loop {
            // Attempt to parse a frame from the buffered data. If enough data
            // has been buffered, the frame is returned.
            if let Some(frame) = self.parse_frame()? {
                return Ok(Some(frame));
            }

            // There is not enough buffered data to read a frame. Attempt to
            // read more data from the socket.
            //
            // On success, the number of bytes is returned. `0` indicates "end
            // of stream".
            let buf = self.buffer.split_off(self.buffer.len());
            let (res, buf) = self.stream.read(buf).await;
            self.buffer.unsplit(buf);

            let read = match res {
                Ok(n) => n,
                Err(_e) => 0,
            };

            if 0 == read {
                // The remote closed the connection. For this to be a clean
                // shutdown, there should be no data in the read buffer. If
                // there is, this means that the peer closed the socket while
                // sending a frame.
                if self.buffer.is_empty() {
                    return Ok(None);
                } else {
                    return Err(FrameError::ConnectionReset);
                }
            }
        }
    }

    /// Tries to parse a frame from the buffer. If the buffer contains enough
    /// data, the frame is returned and the data removed from the buffer. If not
    /// enough data has been buffered yet, `Ok(None)` is returned. If the
    /// buffered data does not represent a valid frame, `Err` is returned.
    fn parse_frame(&mut self) -> Result<Option<Frame>, FrameError> {
        use FrameError::Incomplete;

        // Cursor is used to track the "current" location in the
        // buffer. Cursor also implements `Buf` from the `bytes` crate
        // which provides a number of helpful utilities for working
        // with bytes.
        let mut buf = Cursor::new(&self.buffer[..]);

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
                self.buffer.advance(len);

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
            Err(e) => Err(e.into()),
        }
    }
}
