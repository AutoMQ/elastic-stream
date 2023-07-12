package protocol

import (
	"testing"

	"github.com/brianvoe/gofakeit/v6"
	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/pd/pkg/sbp/codec"
	"github.com/AutoMQ/pd/pkg/util/fbutil"
)

func TestListRangeResponse_Marshal(t *testing.T) {
	var mockListRangeResponse ListRangeResponse
	_ = gofakeit.Struct(&mockListRangeResponse)
	mockData := fbutil.Marshal(&mockListRangeResponse.ListRangeResponseT)
	tests := []struct {
		name    string
		resp    ListRangeResponse
		fmt     codec.Format
		want    []byte
		wantErr bool
		errMsg  string
	}{
		{
			name: "FlatBuffer",
			resp: mockListRangeResponse,
			fmt:  codec.FormatFlatBuffer,
			want: mockData,
		},
		{
			name:    "ProtoBuffer",
			resp:    mockListRangeResponse,
			fmt:     codec.FormatProtoBuffer,
			wantErr: true,
			errMsg:  "unsupported format",
		},
		{
			name:    "JSON",
			resp:    mockListRangeResponse,
			fmt:     codec.FormatJSON,
			wantErr: true,
			errMsg:  "unsupported format",
		},
		{
			name:    "Unknown",
			resp:    mockListRangeResponse,
			fmt:     codec.Format(0),
			wantErr: true,
			errMsg:  "unsupported format",
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			got, err := tt.resp.Marshal(tt.fmt)
			if tt.wantErr {
				re.ErrorContains(err, tt.errMsg)
			} else {
				re.NoError(err)
				re.Equal(tt.want, got)
			}
		})
	}
}
