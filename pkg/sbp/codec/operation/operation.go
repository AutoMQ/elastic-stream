package operation

const (
	// OpUnknown is an unknown operation
	OpUnknown uint16 = 0x0000
	// OpPing measure a minimal round-trip time from the sender.
	OpPing uint16 = 0x0001
	// OpGoAway initiate a shutdown of a connection or signal serious error conditions.
	OpGoAway uint16 = 0x0002
	// OpHeartbeat keep clients alive through periodic heartbeat frames.
	OpHeartbeat uint16 = 0x0003

	// OpAppend append records to the data node.
	OpAppend uint16 = 0x1001
	// OpFetch fetch records from the data node.
	OpFetch uint16 = 0x1002

	// OpListRanges list ranges from the PM of a batch of streams.
	OpListRanges uint16 = 0x2001
	// OpSealRanges request seal ranges of a batch of streams.
	OpSealRanges uint16 = 0x2002
	// OpSyncRanges sync newly writable ranges to a data node to accelerate the availability of a newly created writable range.
	OpSyncRanges uint16 = 0x2003
	// OpDescribeRanges describe the details of a batch of ranges, mainly used to get the max offset of the current writable range.
	OpDescribeRanges uint16 = 0x2004

	// OpCreateStreams create a batch of streams.
	OpCreateStreams uint16 = 0x3001
	// OpDeleteStreams delete a batch of streams.
	OpDeleteStreams uint16 = 0x3002
	// OpUpdateStreams update a batch of streams.
	OpUpdateStreams uint16 = 0x3003
	// OpDescribeStreams fetch the details of a batch of streams.
	OpDescribeStreams uint16 = 0x3004
	// OpTrimStreams trim the min offset of a batch of streams.
	OpTrimStreams uint16 = 0x3005

	// OpReportMetrics report metrics to the PM.
	OpReportMetrics uint16 = 0x4001
)

var (
	_opMap = map[uint16]Operation{
		OpPing:      {OpPing},
		OpGoAway:    {OpGoAway},
		OpHeartbeat: {OpHeartbeat},

		OpAppend: {OpAppend},
		OpFetch:  {OpFetch},

		OpListRanges:     {OpListRanges},
		OpSealRanges:     {OpSealRanges},
		OpSyncRanges:     {OpSyncRanges},
		OpDescribeRanges: {OpDescribeRanges},

		OpCreateStreams:   {OpCreateStreams},
		OpDeleteStreams:   {OpDeleteStreams},
		OpUpdateStreams:   {OpUpdateStreams},
		OpDescribeStreams: {OpDescribeStreams},
		OpTrimStreams:     {OpTrimStreams},

		OpReportMetrics: {OpReportMetrics},
	}
)

// Operation is enumeration of Frame.OpCode
type Operation struct {
	Code uint16
}

// NewOperation new an operation with code
func NewOperation(code uint16) Operation {
	if op, ok := _opMap[code]; ok {
		return op
	}
	return Operation{OpUnknown}
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
	case OpAppend:
		return "Append"
	case OpFetch:
		return "Fetch"
	case OpListRanges:
		return "ListRanges"
	case OpSealRanges:
		return "SealRanges"
	case OpSyncRanges:
		return "SyncRanges"
	case OpDescribeRanges:
		return "DescribeRanges"
	case OpCreateStreams:
		return "CreateStreams"
	case OpDeleteStreams:
		return "DeleteStreams"
	case OpUpdateStreams:
		return "UpdateStreams"
	case OpDescribeStreams:
		return "DescribeStreams"
	case OpTrimStreams:
		return "TrimStreams"
	case OpReportMetrics:
		return "ReportMetrics"
	default:
		return "Unknown"
	}
}
