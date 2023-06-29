package codec

import (
	"bufio"
	"bytes"
	"encoding/binary"
	"fmt"
	"hash/crc32"
	"io"
	"sync/atomic"

	"github.com/bytedance/gopkg/lang/mcache"
	"github.com/pkg/errors"
	"go.uber.org/zap"
	"go.uber.org/zap/zapcore"

	"github.com/AutoMQ/pd/pkg/sbp/codec/format"
	"github.com/AutoMQ/pd/pkg/sbp/codec/operation"
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

	// FlagSystemError indicates whether the response frame is a system error response.
	FlagSystemError Flags = 0x1 << 2
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
	Summarize() []zap.Field

	// Info returns fixed header info of the frame
	Info() []zap.Field

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

func (f baseFrame) Summarize() []zap.Field {
	fields := make([]zapcore.Field, 0, 7)
	fields = append(fields, f.Info()...)
	fields = append(fields, zap.Binary("header", f.Header))
	fields = append(fields, zap.Binary("payload", f.Payload))
	return fields
}

func (f baseFrame) Info() []zap.Field {
	fields := make([]zapcore.Field, 0, 5)
	fields = append(fields, zap.Int("size", f.Size()))
	fields = append(fields, zap.String("operation", f.OpCode.String()))
	fields = append(fields, zap.String("flag", fmt.Sprintf("%08b", f.Flag)))
	fields = append(fields, zap.Uint32("streamID", f.StreamID))
	fields = append(fields, zap.String("format", f.HeaderFmt.String()))
	return fields
}

func (f baseFrame) IsRequest() bool {
	return !f.Flag.Has(FlagResponse)
}

func (f baseFrame) IsResponse() bool {
	return f.Flag.Has(FlagResponse)
}

func (f baseFrame) IsResponseEnd() bool {
	return f.Flag.Has(FlagResponseEnd)
}

func (f baseFrame) IsSystemError() bool {
	return f.Flag.Has(FlagSystemError)
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
// The returned free function is not nil if and only if err is nil. And it should be called after the frame is no longer needed.
func (fr *Framer) ReadFrame() (frame Frame, free func(), err error) {
	logger := fr.lg

	buf := fr.fixedBuf[:_fixedHeaderLen]
	_, err = io.ReadFull(fr.r, buf)
	if err != nil {
		return &baseFrame{}, nil, errors.Wrap(err, "read fixed header")
	}
	headerBuf := bytes.NewBuffer(buf)

	frameLen := binary.BigEndian.Uint32(headerBuf.Next(4))
	if frameLen < _minFrameLen {
		logger.Error("illegal frame length, fewer than minimum", zap.Uint32("frame-length", frameLen), zap.Uint32("min-length", _minFrameLen))
		return &baseFrame{}, nil, errors.Errorf("frame too small: %d < %d", frameLen, _minFrameLen)
	}
	if frameLen > _maxFrameLen {
		logger.Error("illegal frame length, greater than maximum", zap.Uint32("frame-length", frameLen), zap.Uint32("max-length", _maxFrameLen))
		return &baseFrame{}, nil, errors.Errorf("frame too large: %d > %d", frameLen, _maxFrameLen)
	}

	magicCode := headerBuf.Next(1)[0]
	if magicCode != _magicCode {
		logger.Error("illegal magic code", zap.Uint8("expected", _magicCode), zap.Uint8("got", magicCode))
		return &baseFrame{}, nil, errors.Errorf("magic code mismatch: %d != %d", magicCode, _magicCode)
	}

	opCode := binary.BigEndian.Uint16(headerBuf.Next(2))
	flag := headerBuf.Next(1)[0]
	streamID := binary.BigEndian.Uint32(headerBuf.Next(4))
	headerFmt := headerBuf.Next(1)[0]
	headerLen := uint32(headerBuf.Next(1)[0])<<16 | uint32(binary.BigEndian.Uint16(headerBuf.Next(2)))
	payloadLen := frameLen + 4 - _fixedHeaderLen - headerLen - 4 // add frameLength width, sub payloadChecksum width

	tBuf := mcache.Malloc(int(headerLen + payloadLen))
	free = func() { mcache.Free(tBuf) }
	_, err = io.ReadFull(fr.r, tBuf)
	if err != nil {
		free()
		return &baseFrame{}, nil, errors.Wrap(err, "read extended header and payload")
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
		free()
		return &baseFrame{}, nil, errors.Wrap(err, "read payload checksum")
	}
	if payloadLen > 0 {
		if ckm := crc32.ChecksumIEEE(payload); ckm != checksum {
			logger.Error("payload checksum mismatch", zap.Uint32("expected", ckm), zap.Uint32("got", checksum))
			free()
			return &baseFrame{}, nil, errors.Errorf("payload checksum mismatch: %d != %d", ckm, checksum)
		}
	}

	bFrame := baseFrame{
		OpCode:    operation.Operation{Code: opCode},
		Flag:      Flags(flag),
		StreamID:  streamID,
		HeaderFmt: format.NewFormat(headerFmt),
		Header:    header,
		Payload:   payload,
	}
	if logger.Core().Enabled(zapcore.DebugLevel) {
		logger.Debug("read frame", bFrame.Summarize()...)
	}

	switch bFrame.OpCode.Code {
	case operation.OpPing:
		frame = &PingFrame{baseFrame: bFrame}
	case operation.OpGoAway:
		frame = &GoAwayFrame{baseFrame: bFrame}
	default:
		frame = &DataFrame{baseFrame: bFrame}
	}
	return frame, free, nil
}

// WriteFrame writes a frame
//
// It will perform exactly one Write to the underlying Writer.
// It is the caller's responsibility not to violate the maximum frame size
// and to not call other Write methods concurrently.
func (fr *Framer) WriteFrame(f Frame) error {
	logger := fr.lg

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

	err := fr.endWrite()

	if logger.Core().Enabled(zapcore.DebugLevel) {
		fields := f.Summarize()
		fields = append(fields, zap.Error(err))
		logger.Debug("wrote frame", fields...)
	}
	return err
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
	fr.wbuf = fr.wbuf[:0]
	fr.wbuf = binary.BigEndian.AppendUint32(fr.wbuf, 0) // 4 bytes of frame length, will be filled in endWrite
	fr.wbuf = append(fr.wbuf, _magicCode)
	fr.wbuf = binary.BigEndian.AppendUint16(fr.wbuf, frame.OpCode.Code)
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
		return errors.Errorf("frame too large: %d > %d", length, _maxFrameLen)
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
func NewPingFrameResp(ping *PingFrame) (*PingFrame, func()) {
	buf := mcache.Malloc(len(ping.Header) + len(ping.Payload))
	free := func() {
		mcache.Free(buf)
	}
	pong := &PingFrame{baseFrame{
		OpCode:    operation.Operation{Code: operation.OpPing},
		Flag:      FlagResponse | FlagResponseEnd,
		StreamID:  ping.StreamID,
		HeaderFmt: ping.HeaderFmt,
		Header:    buf[:len(ping.Header)],
		Payload:   buf[len(ping.Header):],
	}}
	copy(pong.Header, ping.Header)
	copy(pong.Payload, ping.Payload)
	return pong, free
}

// GoAwayFrame is used to initiate the shutdown of a connection or to signal serious error conditions
type GoAwayFrame struct {
	baseFrame
}

// NewGoAwayFrame creates a new GoAway frame
func NewGoAwayFrame(maxStreamID uint32, isResponse bool) *GoAwayFrame {
	f := &GoAwayFrame{baseFrame{
		OpCode:    operation.Operation{Code: operation.OpGoAway},
		StreamID:  maxStreamID,
		HeaderFmt: format.Default(),
	}}
	if isResponse {
		f.Flag = FlagResponse | FlagResponseEnd
	}
	return f
}

// DataFrame is used to handle other user-defined requests and responses
type DataFrame struct {
	baseFrame
}

// DataFrameContext is the context for DataFrame
type DataFrameContext struct {
	OpCode    operation.Operation
	HeaderFmt format.Format
	StreamID  uint32
}

// NewHeartbeatFrameReq creates a new heartbeat request
func NewHeartbeatFrameReq(streamID uint32, fmt format.Format, header []byte) *DataFrame {
	// treat heartbeat as a special data frame
	return &DataFrame{baseFrame{
		OpCode:    operation.Operation{Code: operation.OpHeartbeat},
		StreamID:  streamID,
		HeaderFmt: fmt,
		Header:    header,
	}}
}

// NewDataFrameReq returns a new DataFrame request
func NewDataFrameReq(context *DataFrameContext, header []byte, payload []byte, flags ...Flags) *DataFrame {
	req := &DataFrame{baseFrame{
		OpCode:    context.OpCode,
		StreamID:  context.StreamID,
		HeaderFmt: context.HeaderFmt,
		Header:    header,
		Payload:   payload,
	}}
	for _, flag := range flags {
		req.Flag |= flag
	}
	return req
}

// NewDataFrameResp returns a new DataFrame response with the given header and payload
func NewDataFrameResp(context *DataFrameContext, header []byte, payload []byte, flags ...Flags) *DataFrame {
	resp := &DataFrame{baseFrame{
		OpCode:    context.OpCode,
		Flag:      FlagResponse,
		StreamID:  context.StreamID,
		HeaderFmt: context.HeaderFmt,
		Header:    header,
		Payload:   payload,
	}}
	for _, flag := range flags {
		resp.Flag |= flag
	}
	return resp
}

// Context returns the context of the DataFrame
func (d *DataFrame) Context() *DataFrameContext {
	return &DataFrameContext{
		OpCode:    d.OpCode,
		HeaderFmt: d.HeaderFmt,
		StreamID:  d.StreamID,
	}
}
