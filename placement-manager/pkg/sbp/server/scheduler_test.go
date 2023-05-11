package server

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/placement-manager/pkg/sbp/codec"
	"github.com/AutoMQ/placement-manager/pkg/sbp/codec/operation"
)

func TestWriteScheduler(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	scheduler := newWriteScheduler()
	scheduler.Push(makeControlWriteRequest(1))
	scheduler.Push(makeDataWriteRequest(2))
	scheduler.Push(makeDataWriteRequest(3))
	scheduler.Push(makeDataWriteRequest(4))
	scheduler.Push(makeControlWriteRequest(5))
	scheduler.Push(makeDataWriteRequest(6))
	scheduler.Push(makeControlWriteRequest(7))
	scheduler.Push(makeDataWriteRequest(3))
	scheduler.Push(makeDataWriteRequest(4))
	scheduler.Push(makeDataWriteRequest(4))
	scheduler.Push(makeControlWriteRequest(8))

	scheduler.CloseStream(3)
	scheduler.CloseStream(3) // close twice is ok

	var order []frameWriteRequest
	for {
		wr, ok := scheduler.Pop()
		if !ok {
			break
		}
		order = append(order, wr)
	}
	re.Len(scheduler.queues, 0)

	re.Len(order, 9)

	re.Equal(makeControlWriteRequest(1), order[0])
	re.Equal(makeControlWriteRequest(5), order[1])
	re.Equal(makeControlWriteRequest(7), order[2])
	re.Equal(makeControlWriteRequest(8), order[3])

	got := make(map[uint32]int)
	for _, wr := range order[4:] {
		got[wr.stream.id]++
	}
	re.Equal(1, got[2])
	re.Equal(3, got[4])
	re.Equal(1, got[6])
}

func makeControlWriteRequest(streamID uint32) frameWriteRequest {
	return frameWriteRequest{
		f: codec.NewGoAwayFrame(streamID, false),
		stream: &stream{
			id: streamID,
		},
	}
}

func makeDataWriteRequest(streamID uint32) frameWriteRequest {
	return frameWriteRequest{
		f: codec.NewDataFrameReq(&codec.DataFrameContext{OpCode: operation.Operation{Code: operation.OpListRange}}, nil, nil),
		stream: &stream{
			id: streamID,
		},
	}
}
