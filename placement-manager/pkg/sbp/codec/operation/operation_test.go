package operation

import (
	"testing"

	"github.com/stretchr/testify/require"
	"go.uber.org/goleak"
)

func TestMain(m *testing.M) {
	goleak.VerifyTestMain(m)
}

func TestNewOperation(t *testing.T) {
	type fields struct {
		code uint16
	}
	type wants struct {
		s    string
		code uint16
	}
	tests := []struct {
		name   string
		fields fields
		wants  wants
	}{
		{
			name:   "Ping",
			fields: fields{code: OpPing},
			wants: wants{
				s:    "Ping",
				code: OpPing,
			},
		},
		{
			name:   "GoAway",
			fields: fields{code: OpGoAway},
			wants: wants{
				s:    "GoAway",
				code: OpGoAway,
			},
		},
		{
			name:   "AllocateID",
			fields: fields{code: OpAllocateID},
			wants: wants{
				s:    "AllocateID",
				code: OpAllocateID,
			},
		},
		{
			name:   "Heartbeat",
			fields: fields{code: OpHeartbeat},
			wants: wants{
				s:    "Heartbeat",
				code: OpHeartbeat,
			},
		},
		{
			name:   "Append",
			fields: fields{code: OpAppend},
			wants: wants{
				s:    "Append",
				code: OpAppend,
			},
		},
		{
			name:   "Fetch",
			fields: fields{code: OpFetch},
			wants: wants{
				s:    "Fetch",
				code: OpFetch,
			},
		},
		{
			name:   "ListRanges",
			fields: fields{code: OpListRanges},
			wants: wants{
				s:    "ListRanges",
				code: OpListRanges,
			},
		},
		{
			name:   "SealRanges",
			fields: fields{code: OpSealRanges},
			wants: wants{
				s:    "SealRanges",
				code: OpSealRanges,
			},
		},
		{
			name:   "SyncRanges",
			fields: fields{code: OpSyncRanges},
			wants: wants{
				s:    "SyncRanges",
				code: OpSyncRanges,
			},
		},
		{
			name:   "DescribeRanges",
			fields: fields{code: OpDescribeRanges},
			wants: wants{
				s:    "DescribeRanges",
				code: OpDescribeRanges,
			},
		},
		{
			name:   "CreateStreams",
			fields: fields{code: OpCreateStreams},
			wants: wants{
				s:    "CreateStreams",
				code: OpCreateStreams,
			},
		},
		{
			name:   "DeleteStreams",
			fields: fields{code: OpDeleteStreams},
			wants: wants{
				s:    "DeleteStreams",
				code: OpDeleteStreams,
			},
		},
		{
			name:   "UpdateStreams",
			fields: fields{code: OpUpdateStreams},
			wants: wants{
				s:    "UpdateStreams",
				code: OpUpdateStreams,
			},
		},
		{
			name:   "DescribeStreams",
			fields: fields{code: OpDescribeStreams},
			wants: wants{
				s:    "DescribeStreams",
				code: OpDescribeStreams,
			},
		},
		{
			name:   "TrimStreams",
			fields: fields{code: OpTrimStreams},
			wants: wants{
				s:    "TrimStreams",
				code: OpTrimStreams,
			},
		},
		{
			name:   "ReadStreams",
			fields: fields{code: OpReportMetrics},
			wants: wants{
				s:    "ReportMetrics",
				code: OpReportMetrics,
			},
		},
		{
			name:   "DescribePMCluster",
			fields: fields{code: OpDescribePMCluster},
			wants: wants{
				s:    "DescribePMCluster",
				code: OpDescribePMCluster,
			},
		},
		{
			name:   "Unknown",
			fields: fields{code: 0},
			wants: wants{
				s:    "Unknown(0x0000)",
				code: 0,
			},
		},
		{
			name:   "Unknown code",
			fields: fields{code: 42},
			wants: wants{
				s:    "Unknown(0x002a)",
				code: 42,
			},
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			o := Operation{tt.fields.code}

			re.Equal(tt.wants.s, o.String())
			re.Equal(tt.wants.code, o.Code)
		})
	}
}

func TestIsControl(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	for _, operation := range []Operation{{OpGoAway}} {
		re.True(operation.IsControl())
	}
	for _, operation := range []Operation{{OpPing}, {OpHeartbeat}, {0}} {
		re.False(operation.IsControl())
	}
}
