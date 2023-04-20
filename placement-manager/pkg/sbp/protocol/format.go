package protocol

import (
	"github.com/pkg/errors"

	"github.com/AutoMQ/placement-manager/pkg/sbp/codec/format"
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
	Marshal(fmt format.Format) ([]byte, error)
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
	return nil, errors.Errorf(_unsupportedFmtErrMsg, format.FlatBuffer())
}

func (m *baseMarshaller) marshalProtoBuffer() ([]byte, error) {
	return nil, errors.Errorf(_unsupportedFmtErrMsg, format.ProtoBuffer())
}

func (m *baseMarshaller) marshalJSON() ([]byte, error) {
	return nil, errors.Errorf(_unsupportedFmtErrMsg, format.JSON())
}

func marshal(m marshaller, fmt format.Format) ([]byte, error) {
	switch fmt {
	case format.FlatBuffer():
		return m.marshalFlatBuffer()
	case format.ProtoBuffer():
		return m.marshalProtoBuffer()
	case format.JSON():
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
	Unmarshal(fmt format.Format, data []byte) error
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
	return errors.Errorf(_unsupportedFmtErrMsg, format.FlatBuffer())
}

func (u *baseUnmarshaler) unmarshalProtoBuffer(_ []byte) error {
	return errors.Errorf(_unsupportedFmtErrMsg, format.ProtoBuffer())
}

func (u *baseUnmarshaler) unmarshalJSON(_ []byte) error {
	return errors.Errorf(_unsupportedFmtErrMsg, format.JSON())
}

func unmarshal(m unmarshaler, fmt format.Format, data []byte) (err error) {
	switch fmt {
	case format.FlatBuffer():
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
	case format.ProtoBuffer():
		return m.unmarshalProtoBuffer(data)
	case format.JSON():
		return m.unmarshalJSON(data)
	default:
		return errors.Errorf(_unsupportedFmtErrMsg, fmt)
	}
}
