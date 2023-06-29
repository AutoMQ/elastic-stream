package operation

import (
	"testing"

	"github.com/stretchr/testify/require"
)

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
			name:   "ListRange",
			fields: fields{code: OpListRange},
			wants: wants{
				s:    "ListRange",
				code: OpListRange,
			},
		},
		{
			name:   "SealRange",
			fields: fields{code: OpSealRange},
			wants: wants{
				s:    "SealRange",
				code: OpSealRange,
			},
		},
		{
			name:   "CreateRange",
			fields: fields{code: OpCreateRange},
			wants: wants{
				s:    "CreateRange",
				code: OpCreateRange,
			},
		},
		{
			name:   "CreateStream",
			fields: fields{code: OpCreateStream},
			wants: wants{
				s:    "CreateStream",
				code: OpCreateStream,
			},
		},
		{
			name:   "DeleteStream",
			fields: fields{code: OpDeleteStream},
			wants: wants{
				s:    "DeleteStream",
				code: OpDeleteStream,
			},
		},
		{
			name:   "UpdateStream",
			fields: fields{code: OpUpdateStream},
			wants: wants{
				s:    "UpdateStream",
				code: OpUpdateStream,
			},
		},
		{
			name:   "DescribeStream",
			fields: fields{code: OpDescribeStream},
			wants: wants{
				s:    "DescribeStream",
				code: OpDescribeStream,
			},
		},
		{
			name:   "TrimStream",
			fields: fields{code: OpTrimStream},
			wants: wants{
				s:    "TrimStream",
				code: OpTrimStream,
			},
		},
		{
			name:   "ReportMetrics",
			fields: fields{code: OpReportMetrics},
			wants: wants{
				s:    "ReportMetrics",
				code: OpReportMetrics,
			},
		},
		{
			name:   "DescribePDCluster",
			fields: fields{code: OpDescribePDCluster},
			wants: wants{
				s:    "DescribePDCluster",
				code: OpDescribePDCluster,
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
