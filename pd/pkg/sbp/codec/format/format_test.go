package format

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestNewFormat(t *testing.T) {
	type fields struct {
		code uint8
	}
	type wants struct {
		s    string
		code uint8
	}
	tests := []struct {
		name   string
		fields fields
		wants  wants
	}{
		{
			name:   "FlatBuffer",
			fields: fields{code: flatBuffer},
			wants: wants{
				s:    "FlatBuffer",
				code: flatBuffer,
			},
		},
		{
			name:   "ProtoBuffer",
			fields: fields{code: protoBuffer},
			wants: wants{
				s:    "ProtoBuffer",
				code: protoBuffer,
			},
		},
		{
			name:   "JSON",
			fields: fields{code: json},
			wants: wants{
				s:    "JSON",
				code: json,
			},
		},
		{
			name:   "Unknown",
			fields: fields{code: unknown},
			wants: wants{
				s:    "Unknown",
				code: unknown,
			},
		},
		{
			name:   "Unknown code",
			fields: fields{code: 42},
			wants: wants{
				s:    "Unknown",
				code: unknown,
			},
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			f := NewFormat(tt.fields.code)

			re.Equal(tt.wants.s, f.String())
			re.Equal(tt.wants.code, f.Code())
		})
	}
}

func TestFormat(t *testing.T) {
	tests := []struct {
		name string
		f    func() Format
		want Format
	}{
		{
			name: "FlatBuffer",
			f:    FlatBuffer,
			want: NewFormat(flatBuffer),
		},
		{
			name: "ProtoBuffer",
			f:    ProtoBuffer,
			want: NewFormat(protoBuffer),
		},
		{
			name: "JSON",
			f:    JSON,
			want: NewFormat(json),
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			re.Equal(tt.want, tt.f())
		})
	}
}
