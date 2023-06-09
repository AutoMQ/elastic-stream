// Code generated by the FlatBuffers compiler. DO NOT EDIT.

package rpcfb

import (
	flatbuffers "github.com/google/flatbuffers/go"
)

type DescribeStreamRequestT struct {
	TimeoutMs int32 `json:"timeout_ms"`
	StreamId int64 `json:"stream_id"`
}

func (t *DescribeStreamRequestT) Pack(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	if t == nil { return 0 }
	DescribeStreamRequestStart(builder)
	DescribeStreamRequestAddTimeoutMs(builder, t.TimeoutMs)
	DescribeStreamRequestAddStreamId(builder, t.StreamId)
	return DescribeStreamRequestEnd(builder)
}

func (rcv *DescribeStreamRequest) UnPackTo(t *DescribeStreamRequestT) {
	t.TimeoutMs = rcv.TimeoutMs()
	t.StreamId = rcv.StreamId()
}

func (rcv *DescribeStreamRequest) UnPack() *DescribeStreamRequestT {
	if rcv == nil { return nil }
	t := &DescribeStreamRequestT{}
	rcv.UnPackTo(t)
	return t
}

type DescribeStreamRequest struct {
	_tab flatbuffers.Table
}

func GetRootAsDescribeStreamRequest(buf []byte, offset flatbuffers.UOffsetT) *DescribeStreamRequest {
	n := flatbuffers.GetUOffsetT(buf[offset:])
	x := &DescribeStreamRequest{}
	x.Init(buf, n+offset)
	return x
}

func GetSizePrefixedRootAsDescribeStreamRequest(buf []byte, offset flatbuffers.UOffsetT) *DescribeStreamRequest {
	n := flatbuffers.GetUOffsetT(buf[offset+flatbuffers.SizeUint32:])
	x := &DescribeStreamRequest{}
	x.Init(buf, n+offset+flatbuffers.SizeUint32)
	return x
}

func (rcv *DescribeStreamRequest) Init(buf []byte, i flatbuffers.UOffsetT) {
	rcv._tab.Bytes = buf
	rcv._tab.Pos = i
}

func (rcv *DescribeStreamRequest) Table() flatbuffers.Table {
	return rcv._tab
}

func (rcv *DescribeStreamRequest) TimeoutMs() int32 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(4))
	if o != 0 {
		return rcv._tab.GetInt32(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *DescribeStreamRequest) MutateTimeoutMs(n int32) bool {
	return rcv._tab.MutateInt32Slot(4, n)
}

func (rcv *DescribeStreamRequest) StreamId() int64 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(6))
	if o != 0 {
		return rcv._tab.GetInt64(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *DescribeStreamRequest) MutateStreamId(n int64) bool {
	return rcv._tab.MutateInt64Slot(6, n)
}

func DescribeStreamRequestStart(builder *flatbuffers.Builder) {
	builder.StartObject(2)
}
func DescribeStreamRequestAddTimeoutMs(builder *flatbuffers.Builder, timeoutMs int32) {
	builder.PrependInt32Slot(0, timeoutMs, 0)
}
func DescribeStreamRequestAddStreamId(builder *flatbuffers.Builder, streamId int64) {
	builder.PrependInt64Slot(1, streamId, 0)
}
func DescribeStreamRequestEnd(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	return builder.EndObject()
}
