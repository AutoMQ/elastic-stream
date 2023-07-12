package codec

import (
	"fmt"
)

type Format uint8

const (
	FormatFlatBuffer Format = iota + 1
	FormatProtoBuffer
	FormatJSON
)

var EnumNamesFormat = map[Format]string{
	FormatFlatBuffer:  "FlatBuffer",
	FormatProtoBuffer: "ProtoBuffer",
	FormatJSON:        "JSON",
}

func (f Format) String() string {
	if s, ok := EnumNamesFormat[f]; ok {
		return s
	}
	return fmt.Sprintf("UnknownFormat(%X)", uint8(f))
}

func DefaultFormat() Format {
	return FormatFlatBuffer
}
