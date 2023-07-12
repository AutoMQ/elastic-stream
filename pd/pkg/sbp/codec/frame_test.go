package codec

import (
	"bytes"
	"testing"

	"github.com/pkg/errors"
	"github.com/stretchr/testify/require"
	"go.uber.org/zap"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
)

func TestNextID(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	fr := NewFramer(nil, nil, nil)

	firstID := fr.NextID()
	secondID := fr.NextID()

	re.Equal(uint32(1), firstID)
	re.Equal(uint32(2), secondID)
}

func TestReadFrame(t *testing.T) {
	tests := []struct {
		name    string
		input   []byte
		want    Frame
		wantErr bool
		errMsg  string
	}{
		{
			name: "normal case",
			input: []byte{
				0x00, 0x00, 0x00, 0x18, // frame length
				0x17,       // magic code
				0x20, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x04, // header length
				0x01, 0x02, 0x03, 0x04, // header data
				0x05, 0x06, 0x07, 0x08, // payload data
				0x53, 0x8D, 0x4D, 0x69, // payload checksum
			},
			want: &DataFrame{baseFrame{
				OpCode:    rpcfb.OperationCodeLIST_RANGE,
				Flag:      3,
				StreamID:  16909060,
				HeaderFmt: FormatFlatBuffer,
				Header:    []byte{0x01, 0x02, 0x03, 0x04},
				Payload:   []byte{0x05, 0x06, 0x07, 0x08},
			}},
		},
		{
			name: "normal case without header",
			input: []byte{
				0x00, 0x00, 0x00, 0x14, // frame length
				0x17,       // magic code
				0x00, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x00, // header length
				0x05, 0x06, 0x07, 0x08, // payload data
				0x53, 0x8D, 0x4D, 0x69, // payload checksum
			},
			want: &PingFrame{baseFrame{
				OpCode:    rpcfb.OperationCodePING,
				Flag:      3,
				StreamID:  16909060,
				HeaderFmt: FormatFlatBuffer,
				Header:    nil,
				Payload:   []byte{0x05, 0x06, 0x07, 0x08},
			}},
		},
		{
			name: "normal case without payload",
			input: []byte{
				0x00, 0x00, 0x00, 0x14, // frame length
				0x17,       // magic code
				0x00, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x04, // header length
				0x01, 0x02, 0x03, 0x04, // header data
				0x00, 0x00, 0x00, 0x00, // payload checksum
			},
			want: &PingFrame{baseFrame{
				OpCode:    rpcfb.OperationCodePING,
				Flag:      3,
				StreamID:  16909060,
				HeaderFmt: FormatFlatBuffer,
				Header:    []byte{0x01, 0x02, 0x03, 0x04},
				Payload:   nil,
			}},
		},
		{
			name: "normal case without header and payload",
			input: []byte{
				0x00, 0x00, 0x00, 0x10, // frame length
				0x17,       // magic code
				0x00, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x00, // header length
				0x00, 0x00, 0x00, 0x00, // payload checksum
			},
			want: &PingFrame{baseFrame{
				OpCode:    rpcfb.OperationCodePING,
				Flag:      3,
				StreamID:  16909060,
				HeaderFmt: FormatFlatBuffer,
				Header:    nil,
				Payload:   nil,
			}},
		},
		{
			name: "not long enough header",
			input: []byte{
				0x00, 0x00, 0x00, 0x18, // frame length
				0x17,       // magic code
				0x00, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
			},
			want:    baseFrame{},
			wantErr: true,
			errMsg:  "read fixed header",
		},
		{
			name: "too small frame",
			input: []byte{
				0x00, 0x00, 0x00, 0x0F, // frame length
				0x17,       // magic code
				0x00, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x04, // header length
			},
			want:    baseFrame{},
			wantErr: true,
			errMsg:  "frame too small",
		},
		{
			name: "too large frame",
			input: []byte{
				0x01, 0x00, 0x00, 0x01, // frame length
				0x17,       // magic code
				0x00, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x00, // header length
				0x00, 0x00, 0x00, 0x00, // payload checksum
			},
			want:    baseFrame{},
			wantErr: true,
			errMsg:  "frame too large",
		},
		{
			name: "mismatched magic code",
			input: []byte{
				0x00, 0x00, 0x00, 0x10, // frame length
				0x00,       // magic code
				0x00, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x00, // header length
				0x00, 0x00, 0x00, 0x00, // payload checksum
			},
			want:    baseFrame{},
			wantErr: true,
			errMsg:  "magic code mismatch",
		},
		{
			name: "not long enough header or payload",
			input: []byte{
				0x00, 0x00, 0x00, 0x18, // frame length
				0x17,       // magic code
				0x20, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x04, // header length
				0x01, 0x02, 0x03, 0x04, // header data
				0x05, 0x06, 0x07, // payload data
			},
			want:    baseFrame{},
			wantErr: true,
			errMsg:  "read extended header and payload",
		},
		{
			name: "not long enough checksum",
			input: []byte{
				0x00, 0x00, 0x00, 0x18, // frame length
				0x17,       // magic code
				0x20, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x04, // header length
				0x01, 0x02, 0x03, 0x04, // header data
				0x05, 0x06, 0x07, 0x08, // payload data
				0x53, 0x8D, 0x4D, // payload checksum
			},
			want:    baseFrame{},
			wantErr: true,
			errMsg:  "read payload checksum",
		},
		{
			name: "mismatched payload checksum",
			input: []byte{
				0x00, 0x00, 0x00, 0x18, // frame length
				0x17,       // magic code
				0x20, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x04, // header length
				0x01, 0x02, 0x03, 0x04, // header data
				0x05, 0x06, 0x07, 0x08, // payload data
				0x69, 0x4D, 0x8D, 0x53, // payload checksum
			},
			want:    baseFrame{},
			wantErr: true,
			errMsg:  "payload checksum mismatch",
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			framer := NewFramer(nil, bytes.NewReader(tt.input), zap.NewNop())
			frame, _, err := framer.ReadFrame()

			if tt.wantErr {
				re.ErrorContains(err, tt.errMsg)
				return
			}
			re.NoError(err)
			zap.NewNop().Info("test", frame.Base().Summarize()...)
			re.Equal(len(tt.input), frame.Base().Size())
			re.Equal(tt.want, frame)
		})
	}
}

func TestWriteFrame(t *testing.T) {
	tests := []struct {
		name    string
		frame   Frame
		want    []byte
		wantErr bool
		errMsg  string
	}{
		{
			name: "normal case",
			frame: DataFrame{baseFrame{
				OpCode:    rpcfb.OperationCodeLIST_RANGE,
				Flag:      3,
				StreamID:  16909060,
				HeaderFmt: FormatFlatBuffer,
				Header:    []byte{0x01, 0x02, 0x03, 0x04},
				Payload:   []byte{0x05, 0x06, 0x07, 0x08},
			}},
			want: []byte{
				0x00, 0x00, 0x00, 0x18, // frame length
				0x17,       // magic code
				0x20, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x04, // header length
				0x01, 0x02, 0x03, 0x04, // header data
				0x05, 0x06, 0x07, 0x08, // payload data
				0x53, 0x8D, 0x4D, 0x69, // payload checksum
			},
		},
		{
			name: "normal case without header",
			frame: PingFrame{baseFrame{
				OpCode:    rpcfb.OperationCodePING,
				Flag:      3,
				StreamID:  16909060,
				HeaderFmt: FormatFlatBuffer,
				Header:    nil,
				Payload:   []byte{0x05, 0x06, 0x07, 0x08},
			}},
			want: []byte{
				0x00, 0x00, 0x00, 0x14, // frame length
				0x17,       // magic code
				0x00, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x00, // header length
				0x05, 0x06, 0x07, 0x08, // payload data
				0x53, 0x8D, 0x4D, 0x69, // payload checksum
			},
		},
		{
			name: "normal case without payload",
			frame: PingFrame{baseFrame{
				OpCode:    rpcfb.OperationCodePING,
				Flag:      3,
				StreamID:  16909060,
				HeaderFmt: FormatFlatBuffer,
				Header:    []byte{0x01, 0x02, 0x03, 0x04},
				Payload:   nil,
			}},
			want: []byte{
				0x00, 0x00, 0x00, 0x14, // frame length
				0x17,       // magic code
				0x00, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x04, // header length
				0x01, 0x02, 0x03, 0x04, // header data
				0x00, 0x00, 0x00, 0x00, // payload checksum
			},
		},
		{
			name: "normal case without header and payload",
			frame: PingFrame{baseFrame{
				OpCode:    rpcfb.OperationCodePING,
				Flag:      3,
				StreamID:  16909060,
				HeaderFmt: FormatFlatBuffer,
				Header:    nil,
				Payload:   nil,
			}},
			want: []byte{
				0x00, 0x00, 0x00, 0x10, // frame length
				0x17,       // magic code
				0x00, 0x01, // op code
				0x03,                   // flag
				0x01, 0x02, 0x03, 0x04, // stream ID
				0x01,             // header format
				0x00, 0x00, 0x00, // header length
				0x00, 0x00, 0x00, 0x00, // payload checksum
			},
		},
		{
			name: "too large payload",
			frame: PingFrame{baseFrame{
				OpCode:    rpcfb.OperationCodePING,
				Flag:      3,
				StreamID:  16909060,
				HeaderFmt: FormatFlatBuffer,
				Header:    nil,
				Payload:   make([]byte, 16*1024*1024+1),
			}},
			want:    []byte{},
			wantErr: true,
			errMsg:  "frame too large",
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			buf := &bytes.Buffer{}
			framer := NewFramer(buf, nil, zap.NewNop())
			err := framer.WriteFrame(tt.frame)

			if tt.wantErr {
				re.ErrorContains(err, tt.errMsg)
				return
			}
			re.NoError(err)
			re.Equal(tt.want, buf.Bytes())
		})
	}
}

type errorWriter struct{}

func (ew *errorWriter) Write([]byte) (n int, err error) {
	return 0, errors.New("mock error")
}

func TestWriteFrameError(t *testing.T) {
	t.Parallel()
	re := require.New(t)
	framer := NewFramer(&errorWriter{}, nil, zap.NewNop())
	err := framer.WriteFrame(baseFrame{
		OpCode:    rpcfb.OperationCodePING,
		Flag:      3,
		StreamID:  16909060,
		HeaderFmt: FormatFlatBuffer,
		Header:    nil,
		Payload:   nil,
	})
	re.ErrorContains(err, "write frame")
}
