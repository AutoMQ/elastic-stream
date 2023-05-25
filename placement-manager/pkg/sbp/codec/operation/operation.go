package operation

import (
	"fmt"
)

const (
	// OpPing measure a minimal round-trip time from the sender.
	OpPing uint16 = 0x0001
	// OpGoAway initiate a shutdown of a connection or signal serious error conditions.
	OpGoAway uint16 = 0x0002

	OpHeartbeat  uint16 = 0x0003
	OpAllocateID uint16 = 0x0004

	OpAppend uint16 = 0x1001
	OpFetch  uint16 = 0x1002

	OpListRange   uint16 = 0x2001
	OpSealRange   uint16 = 0x2002
	OpCreateRange uint16 = 0x2004

	OpCreateStream   uint16 = 0x3001
	OpDeleteStream   uint16 = 0x3002
	OpUpdateStream   uint16 = 0x3003
	OpDescribeStream uint16 = 0x3004
	OpTrimStream     uint16 = 0x3005

	OpReportMetrics     uint16 = 0x4001
	OpDescribePMCluster uint16 = 0x4002
)

// Operation is enumeration of Frame.OpCode
type Operation struct {
	Code uint16
}

// IsControl returns whether o is a control operation
func (o Operation) IsControl() bool {
	switch o.Code {
	case OpGoAway:
		return true
	default:
		return false
	}
}

// String implements fmt.Stringer
func (o Operation) String() string {
	switch o.Code {
	case OpPing:
		return "Ping"
	case OpGoAway:
		return "GoAway"
	case OpHeartbeat:
		return "Heartbeat"
	case OpAllocateID:
		return "AllocateID"
	case OpAppend:
		return "Append"
	case OpFetch:
		return "Fetch"
	case OpListRange:
		return "ListRange"
	case OpSealRange:
		return "SealRange"
	case OpCreateRange:
		return "CreateRange"
	case OpCreateStream:
		return "CreateStream"
	case OpDeleteStream:
		return "DeleteStream"
	case OpUpdateStream:
		return "UpdateStream"
	case OpDescribeStream:
		return "DescribeStream"
	case OpTrimStream:
		return "TrimStream"
	case OpReportMetrics:
		return "ReportMetrics"
	case OpDescribePMCluster:
		return "DescribePMCluster"
	default:
		return fmt.Sprintf("Unknown(%#04x)", o.Code)
	}
}
