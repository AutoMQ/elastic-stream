package protocol

import (
	"github.com/AutoMQ/placement-manager/pkg/sbp/codec/format"
)

var (
	_flatBufferFormatter = newFlatBufferFormatter()
	_unknownFormatter    = unknownFormatter{}
)

type formatter interface {
	unmarshalListRangesRequest([]byte, *ListRangesRequest) error
	marshalListRangesResponse(*ListRangesResponse) ([]byte, error)
	marshalSystemErrorResponse(*SystemErrorResponse) ([]byte, error)
}

func getFormatter(fmt format.Format) formatter {
	switch fmt {
	case format.FlatBuffer():
		return _flatBufferFormatter
	default:
		return _unknownFormatter
	}
}
