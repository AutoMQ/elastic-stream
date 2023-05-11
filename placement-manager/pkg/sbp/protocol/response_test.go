package protocol

import (
	"testing"

	"github.com/brianvoe/gofakeit/v6"
	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/placement-manager/pkg/sbp/codec/format"
	"github.com/AutoMQ/placement-manager/pkg/util/fbutil"
)

func TestListRangeResponse_Marshal(t *testing.T) {
	var mockListRangeResponse ListRangeResponse
	_ = gofakeit.Struct(&mockListRangeResponse)
	mockData := fbutil.Marshal(&mockListRangeResponse.ListRangeResponseT)
	tests := []struct {
		name    string
		resp    ListRangeResponse
		fmt     format.Format
		want    []byte
		wantErr bool
		errMsg  string
	}{
		{
			name: "FlatBuffer",
			resp: mockListRangeResponse,
			fmt:  format.FlatBuffer(),
			want: mockData,
		},
		{
			name:    "ProtoBuffer",
			resp:    mockListRangeResponse,
			fmt:     format.ProtoBuffer(),
			wantErr: true,
			errMsg:  "unsupported format",
		},
		{
			name:    "JSON",
			resp:    mockListRangeResponse,
			fmt:     format.JSON(),
			wantErr: true,
			errMsg:  "unsupported format",
		},
		{
			name:    "Unknown",
			resp:    mockListRangeResponse,
			fmt:     format.NewFormat(0),
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
