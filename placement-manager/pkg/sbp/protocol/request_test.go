package protocol

import (
	"testing"

	"github.com/brianvoe/gofakeit/v6"
	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/placement-manager/pkg/sbp/codec/format"
	"github.com/AutoMQ/placement-manager/pkg/util/fbutil"
)

func TestListRangesRequest_Unmarshal(t *testing.T) {
	var mockListRangesRequest ListRangesRequest
	_ = gofakeit.Struct(&mockListRangesRequest)
	mockData := fbutil.Marshal(&mockListRangesRequest.ListRangesRequestT)

	type args struct {
		fmt  format.Format
		data []byte
	}
	tests := []struct {
		name    string
		args    args
		want    ListRangesRequest
		wantErr bool
		errMsg  string
	}{
		{
			name: "FlatBuffer",
			args: args{
				fmt:  format.FlatBuffer(),
				data: mockData,
			},
			want: mockListRangesRequest,
		},
		{
			name: "ProtoBuffer",
			args: args{
				fmt:  format.ProtoBuffer(),
				data: []byte{},
			},
			wantErr: true,
			errMsg:  "unsupported format",
		},
		{
			name: "JSON",
			args: args{
				fmt:  format.JSON(),
				data: []byte{},
			},
			wantErr: true,
			errMsg:  "unsupported format",
		},
		{
			name: "Unknown",
			args: args{
				fmt:  format.NewFormat(0),
				data: []byte{},
			},
			wantErr: true,
			errMsg:  "unsupported format",
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			l := ListRangesRequest{}
			err := l.Unmarshal(tt.args.fmt, tt.args.data)
			if tt.wantErr {
				re.ErrorContains(err, tt.errMsg)
			} else {
				re.NoError(err)
				re.Equal(tt.want, l)
			}
		})
	}
}
