package protocol

import (
	"github.com/pkg/errors"

	"github.com/AutoMQ/pd/pkg/sbp/codec"
)

const (
	_unsupportedFmtErrMsg = "unsupported format: %s"
)

type marshaller interface {
	flatBufferMarshaller
	protoBufferMarshaller
	jsonMarshaller

	// Marshal encodes the Request (or Response) using the specified format.
	// The returned byte slice is not nil when and only when the error is nil.
	// The returned byte slice should be freed after use.
	Marshal(fmt codec.Format) ([]byte, error)
}

type flatBufferMarshaller interface {
	marshalFlatBuffer() ([]byte, error)
}

type protoBufferMarshaller interface {
	marshalProtoBuffer() ([]byte, error)
}

type jsonMarshaller interface {
	marshalJSON() ([]byte, error)
}

type baseMarshaller struct{}

func (m *baseMarshaller) marshalFlatBuffer() ([]byte, error) {
	return nil, errors.Errorf(_unsupportedFmtErrMsg, codec.FormatFlatBuffer)
}

func (m *baseMarshaller) marshalProtoBuffer() ([]byte, error) {
	return nil, errors.Errorf(_unsupportedFmtErrMsg, codec.FormatProtoBuffer)
}

func (m *baseMarshaller) marshalJSON() ([]byte, error) {
	return nil, errors.Errorf(_unsupportedFmtErrMsg, codec.FormatJSON)
}

func marshal(m marshaller, fmt codec.Format) ([]byte, error) {
	switch fmt {
	case codec.FormatFlatBuffer:
		return m.marshalFlatBuffer()
	case codec.FormatProtoBuffer:
		return m.marshalProtoBuffer()
	case codec.FormatJSON:
		return m.marshalJSON()
	default:
		return nil, errors.Errorf(_unsupportedFmtErrMsg, fmt)
	}
}

type unmarshaler interface {
	flatBufferUnmarshaler
	protoBufferUnmarshaler
	jsonUnmarshaler

	// Unmarshal decodes data into the Request (or Response) using the specified format.
	// data is expired after the call, so the implementation should copy the data if needed.
	Unmarshal(fmt codec.Format, data []byte) error
}

type flatBufferUnmarshaler interface {
	unmarshalFlatBuffer(data []byte) error
}

type protoBufferUnmarshaler interface {
	unmarshalProtoBuffer(data []byte) error
}

type jsonUnmarshaler interface {
	unmarshalJSON(data []byte) error
}

type baseUnmarshaler struct{}

func (u *baseUnmarshaler) unmarshalFlatBuffer(_ []byte) error {
	return errors.Errorf(_unsupportedFmtErrMsg, codec.FormatFlatBuffer)
}

func (u *baseUnmarshaler) unmarshalProtoBuffer(_ []byte) error {
	return errors.Errorf(_unsupportedFmtErrMsg, codec.FormatProtoBuffer)
}

func (u *baseUnmarshaler) unmarshalJSON(_ []byte) error {
	return errors.Errorf(_unsupportedFmtErrMsg, codec.FormatJSON)
}

func unmarshal(m unmarshaler, fmt codec.Format, data []byte) (err error) {
	switch fmt {
	case codec.FormatFlatBuffer:
		defer func() {
			if r := recover(); r != nil {
				switch r := r.(type) {
				case error:
					err = errors.Wrap(r, "unmarshal FlatBuffer")
				default:
					err = errors.Errorf("unmarshal FlatBuffer: %v", r)
				}
			}
		}()
		return m.unmarshalFlatBuffer(data)
	case codec.FormatProtoBuffer:
		return m.unmarshalProtoBuffer(data)
	case codec.FormatJSON:
		return m.unmarshalJSON(data)
	default:
		return errors.Errorf(_unsupportedFmtErrMsg, fmt)
	}
}
