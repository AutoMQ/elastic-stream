use byteorder::ReadBytesExt;
use bytes::{Buf, BufMut, Bytes, BytesMut};
use crc::Crc;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use slog::{trace, warn, Logger};
use std::cell::RefCell;
use std::fmt::{self, format, Display};
use std::io::Cursor;

use crate::error::FrameError;

pub(crate) const MAGIC_CODE: u8 = 23;

pub(crate) const MIN_FRAME_LENGTH: u32 = 16;

// Max frame length 16MB
pub(crate) const MAX_FRAME_LENGTH: u32 = 16 * 1024 * 1024;

const CRC32: Crc<u32> = Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);

thread_local! {
    static STREAM_ID: RefCell<u32> = RefCell::new(1);
}

#[derive(Debug)]
pub struct Frame {
    pub operation_code: OperationCode,

    pub flag: u8,

    // Stream-ID, starting from 1.
    // stream-id `0` is used as placeholder only.
    pub stream_id: u32,

    pub header_format: HeaderFormat,

    pub header: Option<Bytes>,

    pub payload: Option<Bytes>,
}

impl Frame {
    pub fn new(op: OperationCode) -> Self {
        let stream_id = STREAM_ID.with(|f| {
            let mut value = f.borrow_mut();
            let current = *value;
            *value += 1;
            current
        });

        Self {
            operation_code: op,
            flag: 0,
            stream_id,
            header_format: HeaderFormat::FlatBuffer,
            header: None,
            payload: None,
        }
    }

    pub fn flag_response(&mut self) {
        self.flag |= 0x01;
    }

    pub fn flag_end_response(&mut self) {
        self.flag |= 0x03;
    }

    pub fn flag_system_err(&mut self) {
        self.flag |= 0x07;
    }

    fn crc32(payload: &[u8]) -> u32 {
        let mut digest = CRC32.digest();
        digest.update(payload);
        digest.finalize()
    }

    pub fn check(src: &mut Cursor<&[u8]>, logger: &mut Logger) -> Result<(), FrameError> {
        let frame_length = match src.read_u32::<byteorder::NetworkEndian>() {
            Ok(n) => {
                trace!(logger, "Incoming frame length is: {}", n);
                n
            }
            Err(_) => {
                trace!(
                    logger,
                    "Only {} bytes in buffer. Read more data to proceed",
                    src.remaining()
                );
                return Err(FrameError::Incomplete);
            }
        };

        if frame_length < MIN_FRAME_LENGTH {
            warn!(
                logger,
                "Illegal frame length: {}, fewer than minimum: {}", frame_length, MIN_FRAME_LENGTH
            );
            return Err(FrameError::BadFrame(format!(
                "Length of the incoming frame is: {}, less than the minimum possible: {}",
                frame_length, MIN_FRAME_LENGTH
            )));
        }

        // Check if the frame length is legal or not.
        if frame_length > MAX_FRAME_LENGTH {
            warn!(
                logger,
                "Illegal frame length: {}, greater than maximum allowed: {}",
                frame_length,
                MAX_FRAME_LENGTH
            );
            return Err(FrameError::TooLongFrame {
                found: frame_length,
                max: MAX_FRAME_LENGTH,
            });
        }

        // Check if the frame is complete
        if src.remaining() < frame_length as usize {
            trace!(
                logger,
                "Incoming frame length: {}, remaining bytes: {}",
                frame_length,
                src.remaining()
            );
            return Err(FrameError::Incomplete);
        }

        // Verify magic code
        let magic_code = src.get_u8();
        if MAGIC_CODE != magic_code {
            warn!(
                logger,
                "Illegal magic code, expecting: {}, actual: {}", MAGIC_CODE, magic_code
            );
            return Err(FrameError::MagicCodeMismatch {
                found: magic_code,
                expect: MAGIC_CODE,
            });
        }

        // op code
        src.advance(2);

        // flag
        src.advance(1);

        // stream id
        src.advance(4);

        // header format
        src.advance(1);

        // header length
        let header_length: u32 = src.get_u8() as u32;
        let header_length = src.get_u16() as u32 + (header_length << 16);
        if src.remaining() < header_length as usize {
            return Err(FrameError::BadFrame(format!(
                "Header length is: {}, but only {} bytes in buffer",
                header_length,
                src.remaining()
            )));
        }
        src.advance(header_length as usize);

        let mut payload = None;
        if header_length + 16 < frame_length {
            let payload_length = frame_length - header_length - 16;
            if payload_length > src.remaining() as u32 {
                return Err(FrameError::BadFrame(format!(
                    "Payload length is: {}, but only {} bytes in buffer",
                    payload_length,
                    src.remaining()
                )));
            }
            let body = src.copy_to_bytes(payload_length as usize);
            payload = Some(body);
        }

        // Remaining bytes are checksum
        if src.remaining() != 4 {
            return Err(FrameError::BadFrame(
                "The remaining bytes are not checksum".to_string(),
            ));
        }

        if let Some(body) = payload {
            let checksum = src.get_u32();
            let ckm = Frame::crc32(body.as_ref());
            if checksum != ckm {
                warn!(
                    logger,
                    "Payload checksum mismatch. Expecting: {}, Actual: {}", checksum, ckm
                );
                return Err(FrameError::PayloadChecksumMismatch {
                    expected: checksum,
                    actual: ckm,
                });
            }
        } else {
            // checksum
            src.advance(4);
        }

        Ok(())
    }

    pub fn parse(src: &mut Cursor<&[u8]>) -> Result<Frame, FrameError> {
        // Safety: previous `check` method ensures we are having a complete frame to parse
        let frame_length = src.get_u32();
        let mut remaining = frame_length;

        // Skip magic code
        src.advance(1);
        remaining -= 1;

        let op_code = src.get_u16();
        remaining -= 2;
        let op_code = OperationCode::try_from(op_code).unwrap_or(OperationCode::Unknown);

        let flag = src.get_u8();
        remaining -= 1;

        let stream_id = src.get_u32();
        remaining -= 4;

        let header_format = src.get_u8();
        remaining -= 1;
        let header_format = HeaderFormat::try_from(header_format).unwrap_or(HeaderFormat::Unknown);

        let mut frame = Frame {
            operation_code: op_code,
            flag,
            stream_id,
            header_format,
            header: None,
            payload: None,
        };

        let header_length: u32 = src.get_u8() as u32;
        let header_length = src.get_u16() as u32 + (header_length << 16);
        remaining -= 3;

        if header_length > 0 {
            let header = src.copy_to_bytes(header_length as usize);
            frame.header = Some(header);
        }
        remaining -= header_length;

        let payload_length = remaining - 4;
        if payload_length > 0 {
            let payload = src.copy_to_bytes(payload_length as usize);
            frame.payload = Some(payload);
        }
        remaining -= payload_length;

        // payload checksum
        src.advance(4);
        remaining -= 4;
        debug_assert!(0 == remaining);

        Ok(frame)
    }

    pub fn encode(&self, buffer: &mut BytesMut) -> Result<(), FrameError> {
        let mut frame_length = 16;
        if let Some(header) = &self.header {
            frame_length += header.len();
        }

        if let Some(body) = &self.payload {
            frame_length += body.len();
        }

        if frame_length > crate::frame::MAX_FRAME_LENGTH as usize {
            return Err(FrameError::TooLongFrame {
                found: frame_length as u32,
                max: MAX_FRAME_LENGTH,
            });
        }

        // Check to reserve additional memory
        if buffer.capacity() < 4 + frame_length {
            let additional = 4 + frame_length - buffer.capacity();
            buffer.reserve(additional);
        }

        buffer.put_u32(frame_length as u32);
        buffer.put_u8(crate::frame::MAGIC_CODE);
        buffer.put_u16(self.operation_code.into());
        buffer.put_u8(self.flag);
        buffer.put_u32(self.stream_id);
        buffer.put_u8(self.header_format.into());

        if let Some(header) = &self.header {
            let bytes = (header.len() as u32).to_be_bytes();
            debug_assert!(4 == bytes.len());
            buffer.extend_from_slice(&bytes[1..]);
            buffer.extend_from_slice(header.as_ref());
        } else {
            buffer.put_u8(0);
            buffer.put_u16(0);
        }

        if let Some(body) = &self.payload {
            buffer.extend_from_slice(body.as_ref());
            buffer.put_u32(Frame::crc32(body.as_ref()));
        } else {
            // Dummy checksum
            buffer.put_u32(0);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum HeaderFormat {
    Unknown = 0,
    // FlatBuffers format indicates that the payload of the extended header is serialized by flatbuffers.
    // This is the only supported format for now.
    FlatBuffer = 0x01,
    ProtoBuffer = 0x02,
    JSON = 0x03,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u16)]
pub enum OperationCode {
    // 0x0000 is reserved for unknown
    Unknown = 0x0000,

    // 0x0000 ~ 0x0FFF is reserved for system

    // Measure a minimal round-trip time from the sender.
    Ping = 0x0001,
    // Initiate a shutdown of a connection or signal serious error conditions.
    GoAway = 0x0002,
    // To keep clients alive through periodic heartbeat frames.
    Heartbeat = 0x0003,

    // 0x1000 ~ 0x1FFF is reserved for data communication

    // Append records to the data node.
    Append = 0x1001,
    // Fetch records from the data node.
    Fetch = 0x1002,

    // 0x2000 ~ 0x2FFF is reserved for range management

    // List ranges from the PM of a batch of streams.
    ListRanges = 0x2001,
    // Request seal ranges of a batch of streams.
    // The PM will provide the `SEAL_AND_NEW` semantic while Data Node only provide the `SEAL` semantic.
    SealRanges = 0x2002,
    // Syncs newly writable ranges to a data node to accelerate the availability of a newly created writable range.
    SyncRanges = 0x2003,
    // Describe the details of a batch of ranges, mainly used to get the max offset of the current writable range.
    DescribeRanges = 0x2004,

    // 0x3000 ~ 0x3FFF is reserved for stream management

    // Create a batch of streams.
    CreateStreams = 0x3001,
    // Delete a batch of streams.
    DeleteStreams = 0x3002,
    // Update a batch of streams.
    UpdateStreams = 0x3003,
    // Describe the details of a batch of streams.
    DescribeStreams = 0x3004,
    // Trim the min offset of a batch of streams.
    TrimStreams = 0x3005,

    // 0x4000 ~ 0x4FFF is reserved for observability

    // Data node reports metrics to the PM.
    ReportMetrics = 0x4001,
}

impl Display for OperationCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use bytes::{BufMut, BytesMut};

    use super::*;
    use slog::{o, Drain, Logger};

    fn get_logger() -> Logger {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::CompactFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        let log = slog::Logger::root(drain, o!());
        log
    }

    #[test]
    fn test_num_enum() {
        let res = HeaderFormat::try_from(1u8);
        assert_eq!(Ok(HeaderFormat::FlatBuffer), res);

        let num: u8 = HeaderFormat::JSON.into();
        assert_eq!(3, num);

        let res = OperationCode::try_from(0u16);
        assert_eq!(Ok(OperationCode::Unknown), res);
        let num: u16 = OperationCode::GoAway.into();
        assert_eq!(2, num);
    }

    #[test]
    fn test_check() {
        let raw = [1u8];
        let mut rdr = Cursor::new(&raw[..]);
        let mut logger = get_logger();
        let res = Frame::check(&mut rdr, &mut logger);
        assert_eq!(Err(FrameError::Incomplete), res);

        // On read failure, the cursor should be intact.
        assert_eq!(1, rdr.remaining());
    }

    #[test]
    fn test_check_min_frame_length() {
        let mut logger = get_logger();

        let mut buffer = BytesMut::new();
        buffer.put_u32(10);

        let mut cursor = Cursor::new(&buffer[..]);
        match Frame::check(&mut cursor, &mut logger) {
            Ok(_) => {
                panic!("Should have detected the frame length issue");
            }
            Err(e) => {
                assert_eq!(
                    FrameError::BadFrame(
                        "Length of the incoming frame is: 10, less than the minimum possible: 16"
                            .to_owned()
                    ),
                    e
                );
            }
        }
    }

    #[test]
    fn test_check_max_frame_length() {
        let mut buffer = BytesMut::new();
        buffer.put_u32(MAX_FRAME_LENGTH + 1);
        let mut logger = get_logger();

        let mut cursor = Cursor::new(&buffer[..]);
        match Frame::check(&mut cursor, &mut logger) {
            Ok(_) => {
                panic!("Should have detected the frame length issue");
            }
            Err(e) => {
                assert_eq!(
                    FrameError::TooLongFrame {
                        found: MAX_FRAME_LENGTH + 1,
                        max: MAX_FRAME_LENGTH
                    },
                    e
                );
            }
        }
    }

    #[test]
    fn test_check_magic_code() {
        let mut buffer = BytesMut::new();
        buffer.put_u32(MIN_FRAME_LENGTH);
        // magic code
        buffer.put_u8(16u8);
        // operation code
        buffer.put_u16(OperationCode::Ping.into());
        // flag
        buffer.put_u8(0u8);
        // stream identifier
        buffer.put_u32(2);
        // header format + header length
        buffer.put_u32(0);
        // header
        // payload
        // payload checksum
        buffer.put_u32(0);

        let mut cursor = Cursor::new(&buffer[..]);

        let mut logger = get_logger();
        match Frame::check(&mut cursor, &mut logger) {
            Ok(_) => {
                panic!("Should have detected the frame magic code mismatch issue");
            }
            Err(e) => {
                assert_eq!(
                    FrameError::MagicCodeMismatch {
                        found: 16u8,
                        expect: MAGIC_CODE
                    },
                    e
                );
            }
        }
    }

    #[test]
    fn test_encode_header() {
        let mut header = BytesMut::with_capacity(16);
        header.put(&b"abc"[..]);

        let frame = Frame {
            operation_code: OperationCode::Ping,
            flag: 1,
            stream_id: 2,
            header_format: HeaderFormat::FlatBuffer,
            header: Some(header.freeze()),
            payload: None,
        };

        let mut buf = BytesMut::new();

        assert_eq!(Ok(()), frame.encode(&mut buf));

        let frame_length = buf.get_u32();
        assert_eq!(19, frame_length);
        assert_eq!(MAGIC_CODE, buf.get_u8());
        assert_eq!(1, buf.get_u16());
        assert_eq!(1, buf.get_u8());
        assert_eq!(2, buf.get_u32());
        assert_eq!(1, buf.get_u8());
        // header length
        assert_eq!(0, buf.get_u8());
        assert_eq!(3, buf.get_u16());

        let header = buf.copy_to_bytes(3);
        assert_eq!(b"abc", header.as_ref());

        assert_eq!(0, buf.get_u32());

        assert_eq!(0, buf.remaining());
    }

    #[test]
    fn test_encode_body() {
        let mut body = BytesMut::with_capacity(16);
        body.put(&b"abc"[..]);

        let frame = Frame {
            operation_code: OperationCode::Ping,
            flag: 1,
            stream_id: 2,
            header_format: HeaderFormat::FlatBuffer,
            header: None,
            payload: Some(body.freeze()),
        };

        let mut buf = BytesMut::new();
        assert_eq!(Ok(()), frame.encode(&mut buf));

        assert_eq!(19, buf.get_u32());

        assert_eq!(MAGIC_CODE, buf.get_u8());
        assert_eq!(1, buf.get_u16());
        assert_eq!(1, buf.get_u8());
        assert_eq!(2, buf.get_u32());
        assert_eq!(1, buf.get_u8());
        // header length
        assert_eq!(0, buf.get_u8());
        assert_eq!(0, buf.get_u16());

        let body = buf.copy_to_bytes(3);
        assert_eq!(b"abc", body.as_ref());
        // checksum
        assert_eq!(Frame::crc32(b"abc"), buf.get_u32());
        assert_eq!(0, buf.remaining());
        assert_eq!(0, buf.len());
    }

    #[test]
    fn test_too_long_header_length() {
        let mut raw_frame = BytesMut::with_capacity(16);
        // frame length
        raw_frame.put_u32(19);
        // magic code
        raw_frame.put_u8(MAGIC_CODE);
        // operation code
        raw_frame.put_u16(OperationCode::Ping.into());
        // flag
        raw_frame.put_u8(1);
        // stream identifier
        raw_frame.put_u32(2);
        // header format + header length
        raw_frame.put_u8(HeaderFormat::FlatBuffer.into());
        // Set a header length that is too long
        raw_frame.extend_from_slice((1024_i32).to_be_bytes()[1..].as_ref());
        // header
        raw_frame.put(&b"abc"[..]);
        // empty payload
        // payload checksum
        raw_frame.put_u32(0);

        let mut cursor = Cursor::new(&raw_frame[..]);
        let mut logger = get_logger();

        match Frame::check(&mut cursor, &mut logger) {
            Ok(_) => {
                panic!("Should have detected the frame header length issue");
            }
            Err(e) => {
                assert!(matches!(e, FrameError::BadFrame { .. }));
            }
        }
    }

    #[test]
    fn test_bad_frame_no_checksum() {
        let mut raw_frame = BytesMut::with_capacity(16);
        // frame length
        raw_frame.put_u32(25);
        // magic code
        raw_frame.put_u8(MAGIC_CODE);
        // operation code
        raw_frame.put_u16(OperationCode::Ping.into());
        // flag
        raw_frame.put_u8(1);
        // stream identifier
        raw_frame.put_u32(2);
        // header format + header length
        raw_frame.put_u8(HeaderFormat::FlatBuffer.into());
        // header length
        raw_frame.extend_from_slice((10_i32).to_be_bytes()[1..].as_ref());
        // header
        raw_frame.put(&b"header"[..]);
        // payload length
        // payload
        raw_frame.put(&b"abc"[..]);
        // payload checksum
        raw_frame.put_u32(0);

        let mut cursor = Cursor::new(&raw_frame[..]);
        let mut logger = get_logger();

        match Frame::check(&mut cursor, &mut logger) {
            Ok(_) => {
                panic!("Should have detected the frame payload length issue");
            }
            Err(e) => {
                assert!(matches!(e, FrameError::BadFrame { .. }));
            }
        }
    }
    #[test]
    fn test_check_and_parse() {
        let mut header = BytesMut::new();
        header.put(&b"header"[..]);

        let mut body = BytesMut::with_capacity(16);
        body.put(&b"abc"[..]);

        let frame = Frame {
            operation_code: OperationCode::Ping,
            flag: 1,
            stream_id: 2,
            header_format: HeaderFormat::FlatBuffer,
            header: Some(header.freeze()),
            payload: Some(body.freeze()),
        };

        let mut buf = BytesMut::new();
        assert_eq!(Ok(()), frame.encode(&mut buf));
        assert_eq!(29, buf.remaining());

        let mut cursor = Cursor::new(&buf[..]);

        let mut logger = get_logger();

        // Frame::check should pass
        assert_eq!(Ok(()), Frame::check(&mut cursor, &mut logger));

        // Reset cursor
        cursor.set_position(0);

        // Validate parse
        let decoded = Frame::parse(&mut cursor).unwrap();
        assert_eq!(OperationCode::Ping, decoded.operation_code);
        assert_eq!(1, decoded.flag);
        assert_eq!(2, decoded.stream_id);
        assert_eq!(HeaderFormat::FlatBuffer, decoded.header_format);
        assert_eq!(Some(Bytes::from("header")), decoded.header);
        assert_eq!(Some(Bytes::from("abc")), decoded.payload);
    }
}
