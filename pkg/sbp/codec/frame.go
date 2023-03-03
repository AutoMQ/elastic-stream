package codec

import (
	"bufio"
	"bytes"
	"encoding/binary"
	"fmt"
	"hash/crc32"
	"io"
	"sync/atomic"

	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/pkg/sbp/format"
	"github.com/AutoMQ/placement-manager/pkg/sbp/operation"
)

const (
	_fixedHeaderLen = 16
	_minFrameLen    = _fixedHeaderLen - 4 + 4 // fixed header - header length + checksum
	_maxFrameLen    = 16 * 1024 * 1024

	_magicCode uint8 = 23
)

const (
	// FlagResponse indicates whether the frame is a response frame.
	// If set, the frame contains the response payload to a specific request frame identified by a stream identifier.
	// If not set, the frame represents a request frame.
	FlagResponse Flags = 0x1

	// FlagResponseEnd indicates whether the response frame is the last frame of the response.
	// If set, the frame is the last frame in a response sequence.
	// If not set, the response sequence continues with more frames.
	FlagResponseEnd Flags = 0x1 << 1
)

// Flags is a bitmask of SBP flags.
type Flags uint8

// Has reports whether f contains all (0 or more) flags in v.
func (f Flags) Has(v Flags) bool {
	return (f & v) == v
}

// Frame is the base interface implemented by all frame types
type Frame interface {
	Base() baseFrame

	// Size returns the number of bytes that the Frame takes after encoding
	Size() int

	// Summarize returns all info of the frame, only for debug use
	Summarize() string

	// Info returns fixed header info of the frame
	Info() string

	// IsRequest returns whether the frame is a request
	IsRequest() bool

	// IsResponse returns whether the frame is a response
	IsResponse() bool
}

// baseFrame is the load in SBP.
//
//	+-----------------------------------------------------------------------+
//	|                           Frame Length (32)                           |
//	+-----------------+-----------------------------------+-----------------+
//	|  Magic Code (8) |        Operation Code (16)        |     Flag (8)    |
//	+-----------------+-----------------------------------+-----------------+
//	|                         Stream Identifier (32)                        |
//	+-----------------+-----------------------------------------------------+
//	|Header Format (8)|                  Header Length (24)                 |
//	+-----------------+-----------------------------------------------------+
//	|                             Header (0...)                           ...
//	+-----------------------------------------------------------------------+
//	|                             Payload (0...)                          ...
//	+-----------------------------------------------------------------------+
//	|                         Payload Checksum (32)                         |
//	+-----------------------------------------------------------------------+
type baseFrame struct {
	OpCode    operation.Operation // OpCode determines the format and semantics of the frame
	Flag      Flags               // Flag is reserved for boolean flags specific to the frame type
	StreamID  uint32              // StreamID identifies which stream the frame belongs to
	HeaderFmt format.Format       // HeaderFmt identifies the format of the Header.
	Header    []byte              // nil for no extended header
	Payload   []byte              // nil for no payload
}

// Base implement the Frame interface
func (f baseFrame) Base() baseFrame {
	return f
}

func (f baseFrame) Size() int {
	return _fixedHeaderLen + len(f.Header) + len(f.Payload) + 4
}

func (f baseFrame) Summarize() string {
	var buf bytes.Buffer
	buf.WriteString(f.Info())
	_, _ = fmt.Fprintf(&buf, " header=%q", f.Header)
	payload := f.Payload
	const max = 256
	if len(payload) > max {
		payload = payload[:max]
	}
	_, _ = fmt.Fprintf(&buf, " payload=%q", payload)
	if len(f.Payload) > max {
		_, _ = fmt.Fprintf(&buf, " (%d bytes omitted)", len(f.Payload)-max)
	}
	return buf.String()
}

func (f baseFrame) Info() string {
	var buf bytes.Buffer
	_, _ = fmt.Fprintf(&buf, "size=%d", f.Size())
	_, _ = fmt.Fprintf(&buf, " operation=%s", f.OpCode.String())
	_, _ = fmt.Fprintf(&buf, " flag=%08b", f.Flag)
	_, _ = fmt.Fprintf(&buf, " streamID=%d", f.StreamID)
	_, _ = fmt.Fprintf(&buf, " format=%s", f.HeaderFmt.String())
	return buf.String()
}

func (f baseFrame) IsRequest() bool {
	return !f.Flag.Has(FlagResponse)
}

func (f baseFrame) IsResponse() bool {
	return f.Flag.Has(FlagResponse)
}

// Framer reads and writes Frames
type Framer struct {
	streamID atomic.Uint32

	r io.Reader
	// fixedBuf is used to cache the fixed length portion in the frame
	fixedBuf [_fixedHeaderLen]byte

	w    io.Writer
	wbuf []byte

	lg *zap.Logger
}

// NewFramer returns a Framer that writes frames to w and reads them from r
func NewFramer(w io.Writer, r io.Reader, logger *zap.Logger) *Framer {
	framer := &Framer{
		w:  w,
		r:  r,
		lg: logger,
	}
	framer.streamID = atomic.Uint32{}
	return framer
}

// NextID generates the next new StreamID
func (fr *Framer) NextID() uint32 {
	return fr.streamID.Add(1)
}

// ReadFrame reads a single frame
func (fr *Framer) ReadFrame() (Frame, error) {
	logger := fr.lg

	buf := fr.fixedBuf[:_fixedHeaderLen]
	_, err := io.ReadFull(fr.r, buf)
	if err != nil {
		logger.Error("failed to read fixed header", zap.Error(err))
		return &baseFrame{}, errors.Wrap(err, "read fixed header")
	}
	headerBuf := bytes.NewBuffer(buf)

	frameLen := binary.BigEndian.Uint32(headerBuf.Next(4))
	if frameLen < _minFrameLen {
		logger.Error("illegal frame length, fewer than minimum", zap.Uint32("frame-length", frameLen), zap.Uint32("min-length", _minFrameLen))
		return &baseFrame{}, errors.New("frame too small")
	}
	if frameLen > _maxFrameLen {
		logger.Error("illegal frame length, greater than maximum", zap.Uint32("frame-length", frameLen), zap.Uint32("max-length", _maxFrameLen))
		return &baseFrame{}, errors.New("frame too large")
	}

	magicCode := headerBuf.Next(1)[0]
	if magicCode != _magicCode {
		logger.Error("illegal magic code", zap.Uint8("expected", _magicCode), zap.Uint8("got", magicCode))
		return &baseFrame{}, errors.New("magic code mismatch")
	}

	opCode := binary.BigEndian.Uint16(headerBuf.Next(2))
	flag := headerBuf.Next(1)[0]
	streamID := binary.BigEndian.Uint32(headerBuf.Next(4))
	headerFmt := headerBuf.Next(1)[0]
	headerLen := uint32(headerBuf.Next(1)[0])<<16 | uint32(binary.BigEndian.Uint16(headerBuf.Next(2)))
	payloadLen := frameLen + 4 - _fixedHeaderLen - headerLen - 4 // add frameLength width, sub payloadChecksum width

	// TODO malloc and free buffer by github.com/bytedance/gopkg/lang/mcache
	tBuf := make([]byte, headerLen+payloadLen)
	_, err = io.ReadFull(fr.r, tBuf)
	if err != nil {
		logger.Error("failed to read extended header and payload", zap.Error(err))
		return &baseFrame{}, errors.Wrap(err, "read extended header and payload")
	}

	header := func() []byte {
		if headerLen == 0 {
			return nil
		}
		return tBuf[:headerLen]
	}()
	payload := func() []byte {
		if payloadLen == 0 {
			return nil
		}
		return tBuf[headerLen:]
	}()

	var checksum uint32
	err = binary.Read(fr.r, binary.BigEndian, &checksum)
	if err != nil {
		logger.Error("failed to read payload checksum", zap.Error(err))
		return &baseFrame{}, errors.Wrap(err, "read payload checksum")
	}
	if payloadLen > 0 {
		if ckm := crc32.ChecksumIEEE(payload); ckm != checksum {
			logger.Error("payload checksum mismatch", zap.Uint32("expected", ckm), zap.Uint32("got", checksum))
			return &baseFrame{}, errors.New("payload checksum mismatch")
		}
	}

	bFrame := baseFrame{
		OpCode:    operation.NewOperation(opCode),
		Flag:      Flags(flag),
		StreamID:  streamID,
		HeaderFmt: format.NewFormat(headerFmt),
		Header:    header,
		Payload:   payload,
	}

	var frame Frame
	switch bFrame.OpCode {
	case operation.Ping():
		frame = &PingFrame{baseFrame: bFrame}
	case operation.GoAway():
		frame = &GoAwayFrame{baseFrame: bFrame}
	case operation.Heartbeat():
		frame = &HeartbeatFrame{baseFrame: bFrame}
	case operation.Publish(), operation.ListRange():
		frame = &DataFrame{baseFrame: bFrame}
	default:
		frame = &bFrame
	}
	return frame, nil
}

// WriteFrame writes a frame
//
// It will perform exactly one Write to the underlying Writer.
// It is the caller's responsibility not to violate the maximum frame size
// and to not call other Write methods concurrently.
func (fr *Framer) WriteFrame(f Frame) error {
	frame := f.Base()
	fr.startWrite(frame)

	if frame.Header != nil {
		fr.wbuf = append(fr.wbuf, frame.Header...)
	}
	if frame.Payload != nil {
		fr.wbuf = append(fr.wbuf, frame.Payload...)
		fr.wbuf = binary.BigEndian.AppendUint32(fr.wbuf, crc32.ChecksumIEEE(frame.Payload))
	} else {
		// dummy checksum
		fr.wbuf = binary.BigEndian.AppendUint32(fr.wbuf, 0)
	}

	return fr.endWrite()
}

// Flush writes any buffered data to the underlying io.Writer.
func (fr *Framer) Flush() error {
	if bw, ok := fr.w.(*bufio.Writer); ok {
		return bw.Flush()
	}
	return nil
}

// Available returns how many bytes are unused in the buffer.
func (fr *Framer) Available() int {
	if bw, ok := fr.w.(*bufio.Writer); ok {
		return bw.Available()
	}
	return 0
}

// Write the fixed header
func (fr *Framer) startWrite(frame baseFrame) {
	fr.wbuf = binary.BigEndian.AppendUint32(fr.wbuf, 0) // 4 bytes of frame length, will be filled in endWrite
	fr.wbuf = append(fr.wbuf, _magicCode)
	fr.wbuf = binary.BigEndian.AppendUint16(fr.wbuf, frame.OpCode.Code())
	fr.wbuf = append(fr.wbuf, uint8(frame.Flag))
	fr.wbuf = binary.BigEndian.AppendUint32(fr.wbuf, frame.StreamID)
	fr.wbuf = append(fr.wbuf, frame.HeaderFmt.Code())
	headerLen := len(frame.Header)
	fr.wbuf = append(fr.wbuf, byte(headerLen>>16), byte(headerLen>>8), byte(headerLen))
}

func (fr *Framer) endWrite() error {
	logger := fr.lg
	// Now that we know the final size, fill in the FrameHeader in
	// the space previously reserved for it. Abuse append.
	length := len(fr.wbuf) - 4 // sub frameLen width
	if length > (_maxFrameLen) {
		logger.Error("frame too large, greater than maximum", zap.Int("frame-length", length), zap.Uint32("max-length", _maxFrameLen))
		return errors.New("frame too large")
	}
	_ = binary.BigEndian.AppendUint32(fr.wbuf[:0], uint32(length))

	_, err := fr.w.Write(fr.wbuf)
	if err != nil {
		logger.Error("failed to write frame", zap.Error(err))
		return errors.Wrap(err, "write frame")
	}
	return nil
}

// PingFrame is a mechanism for measuring a minimal round-trip time from the sender,
// as well as determining whether an idle connection is still functional
type PingFrame struct {
	baseFrame
}

// NewPingFrameResp creates a pong with the provided ping
func NewPingFrameResp(ping *PingFrame) *PingFrame {
	pong := &PingFrame{baseFrame{
		OpCode:    operation.Ping(),
		Flag:      FlagResponse | FlagResponseEnd,
		StreamID:  ping.StreamID,
		HeaderFmt: ping.HeaderFmt,
		Header:    make([]byte, len(ping.Header)),
		Payload:   make([]byte, len(ping.Payload)),
	}}
	copy(pong.Header, ping.Header)
	copy(pong.Payload, ping.Payload)
	return pong
}

// GoAwayFrame is used to initiate the shutdown of a connection or to signal serious error conditions
type GoAwayFrame struct {
	baseFrame
}

// NewGoAwayFrameReq creates a new GoAway frame
func NewGoAwayFrameReq(streamID uint32) *GoAwayFrame {
	return &GoAwayFrame{baseFrame{
		OpCode:    operation.GoAway(),
		StreamID:  streamID,
		HeaderFmt: format.Default(),
	}}
}

// HeartbeatFrame is used to keep clients alive
type HeartbeatFrame struct {
	baseFrame
}

// NewHeartBeatFrameResp creates an out heartbeat with the in heartbeat
func NewHeartBeatFrameResp(in *HeartbeatFrame) *HeartbeatFrame {
	out := &HeartbeatFrame{baseFrame{
		OpCode:    operation.Heartbeat(),
		Flag:      FlagResponse | FlagResponseEnd,
		StreamID:  in.StreamID,
		HeaderFmt: in.HeaderFmt,
		Header:    make([]byte, len(in.Header)),
	}}
	copy(in.Header, out.Header)
	return out
}

// DataFrame is used to handle other user-defined requests and responses
type DataFrame struct {
	baseFrame
}

// DataFrameReqParam is used to create a new DataFrame request
type DataFrameReqParam struct {
	OpCode    operation.Operation
	HeaderFmt format.Format
	Header    []byte
	Payload   []byte
}

// NewDataFrameReq returns a new DataFrame request
func NewDataFrameReq(req DataFrameReqParam, flag Flags, streamID uint32) *DataFrame {
	return &DataFrame{baseFrame{
		OpCode:    req.OpCode,
		Flag:      flag,
		StreamID:  streamID,
		HeaderFmt: req.HeaderFmt,
		Header:    req.Header,
		Payload:   req.Payload,
	}}
}

// NewDataFrameResp returns a new DataFrame response with the given header and payload
func NewDataFrameResp(req *DataFrame, header []byte, payload []byte, isEnd bool) *DataFrame {
	resp := &DataFrame{baseFrame{
		OpCode:    req.OpCode,
		Flag:      FlagResponse,
		StreamID:  req.StreamID,
		HeaderFmt: req.HeaderFmt,
		Header:    header,
		Payload:   payload,
	}}
	if isEnd {
		resp.Flag |= FlagResponseEnd
	}
	return resp
}
