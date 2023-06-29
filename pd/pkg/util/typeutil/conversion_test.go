package typeutil

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestBytesToUint64(t *testing.T) {
	type args struct {
		b []byte
	}
	tests := []struct {
		name    string
		args    args
		want    uint64
		wantErr bool
	}{
		{
			name: "zero",
			args: args{
				b: []byte{0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00},
			},
			want:    0,
			wantErr: false,
		},
		{
			name: "a random number",
			args: args{
				b: []byte{0x11, 0x22, 0x10, 0xF4, 0x7D, 0xE9, 0x81, 0x15},
			},
			want:    1234567890123456789,
			wantErr: false,
		},
		{
			name: "error when length does not match",
			args: args{
				b: []byte{0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00},
			},
			want:    0,
			wantErr: true,
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			got, err := BytesToUint64(tt.args.b)

			if tt.wantErr {
				re.Error(err)
			} else {
				re.NoError(err)
				re.Equal(tt.want, got)
			}
		})
	}
}

func TestUint64ToBytes(t *testing.T) {
	type args struct {
		v uint64
	}
	tests := []struct {
		name string
		args args
		want []byte
	}{
		{
			name: "zero",
			args: args{
				v: 0,
			},
			want: []byte{0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00},
		},
		{
			name: "a random number",
			args: args{
				v: 1234567890123456789,
			},
			want: []byte{0x11, 0x22, 0x10, 0xF4, 0x7D, 0xE9, 0x81, 0x15},
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			got := Uint64ToBytes(tt.args.v)

			re.Equal(tt.want, got)
		})
	}
}
