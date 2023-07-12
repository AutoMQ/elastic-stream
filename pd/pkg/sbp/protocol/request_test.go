package protocol

import (
	"testing"

	"github.com/brianvoe/gofakeit/v6"
	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/pd/pkg/sbp/codec"
	"github.com/AutoMQ/pd/pkg/util/fbutil"
)

func TestListRangeRequest_Unmarshal(t *testing.T) {
	var mockListRangeRequest ListRangeRequest
	_ = gofakeit.Struct(&mockListRangeRequest)
	mockData := fbutil.Marshal(&mockListRangeRequest.ListRangeRequestT)

	type args struct {
		fmt  codec.Format
		data []byte
	}
	tests := []struct {
		name    string
		args    args
		want    ListRangeRequest
		wantErr bool
		errMsg  string
	}{
		{
			name: "FlatBuffer",
			args: args{
				fmt:  codec.FormatFlatBuffer,
				data: mockData,
			},
			want: mockListRangeRequest,
		},
		{
			name: "FlatBuffer in wrong format",
			args: args{
				fmt:  codec.FormatFlatBuffer,
				data: []byte{'a', 'b', 'c'},
			},
			wantErr: true,
			errMsg:  "unmarshal FlatBuffer:",
		},
		{
			name: "ProtoBuffer",
			args: args{
				fmt:  codec.FormatProtoBuffer,
				data: []byte{},
			},
			wantErr: true,
			errMsg:  "unsupported format",
		},
		{
			name: "JSON",
			args: args{
				fmt:  codec.FormatJSON,
				data: []byte{},
			},
			wantErr: true,
			errMsg:  "unsupported format",
		},
		{
			name: "Unknown",
			args: args{
				fmt:  codec.Format(0),
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

			l := ListRangeRequest{}
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
