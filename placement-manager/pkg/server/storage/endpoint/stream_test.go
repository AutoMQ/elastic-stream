package endpoint

import (
	"math"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestStreamPath(t *testing.T) {
	tests := []struct {
		name     string
		streamID int64
		want     string
	}{
		{
			name:     "zero",
			streamID: 0,
			want:     "streams/00000000000000000000",
		},
		{
			name:     "positive",
			streamID: 123456789,
			want:     "streams/00000000000123456789",
		},
		{
			name:     "max",
			streamID: math.MaxInt64,
			want:     "streams/09223372036854775807",
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			re.Equal(tt.want, string(streamPath(tt.streamID)))
		})
	}
}
