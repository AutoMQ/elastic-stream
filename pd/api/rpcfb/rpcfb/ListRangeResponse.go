// Code generated by the FlatBuffers compiler. DO NOT EDIT.

package rpcfb

import (
	flatbuffers "github.com/google/flatbuffers/go"
)

type ListRangeResponseT struct {
	Status *StatusT `json:"status"`
	ThrottleTimeMs int32 `json:"throttle_time_ms"`
	Ranges []*RangeT `json:"ranges"`
}

func (t *ListRangeResponseT) Pack(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	if t == nil { return 0 }
	statusOffset := t.Status.Pack(builder)
	rangesOffset := flatbuffers.UOffsetT(0)
	if t.Ranges != nil {
		rangesLength := len(t.Ranges)
		rangesOffsets := make([]flatbuffers.UOffsetT, rangesLength)
		for j := 0; j < rangesLength; j++ {
			rangesOffsets[j] = t.Ranges[j].Pack(builder)
		}
		ListRangeResponseStartRangesVector(builder, rangesLength)
		for j := rangesLength - 1; j >= 0; j-- {
			builder.PrependUOffsetT(rangesOffsets[j])
		}
		rangesOffset = builder.EndVector(rangesLength)
	}
	ListRangeResponseStart(builder)
	ListRangeResponseAddStatus(builder, statusOffset)
	ListRangeResponseAddThrottleTimeMs(builder, t.ThrottleTimeMs)
	ListRangeResponseAddRanges(builder, rangesOffset)
	return ListRangeResponseEnd(builder)
}

func (rcv *ListRangeResponse) UnPackTo(t *ListRangeResponseT) {
	t.Status = rcv.Status(nil).UnPack()
	t.ThrottleTimeMs = rcv.ThrottleTimeMs()
	rangesLength := rcv.RangesLength()
	t.Ranges = make([]*RangeT, rangesLength)
	for j := 0; j < rangesLength; j++ {
		x := Range{}
		rcv.Ranges(&x, j)
		t.Ranges[j] = x.UnPack()
	}
}

func (rcv *ListRangeResponse) UnPack() *ListRangeResponseT {
	if rcv == nil { return nil }
	t := &ListRangeResponseT{}
	rcv.UnPackTo(t)
	return t
}

type ListRangeResponse struct {
	_tab flatbuffers.Table
}

func GetRootAsListRangeResponse(buf []byte, offset flatbuffers.UOffsetT) *ListRangeResponse {
	n := flatbuffers.GetUOffsetT(buf[offset:])
	x := &ListRangeResponse{}
	x.Init(buf, n+offset)
	return x
}

func GetSizePrefixedRootAsListRangeResponse(buf []byte, offset flatbuffers.UOffsetT) *ListRangeResponse {
	n := flatbuffers.GetUOffsetT(buf[offset+flatbuffers.SizeUint32:])
	x := &ListRangeResponse{}
	x.Init(buf, n+offset+flatbuffers.SizeUint32)
	return x
}

func (rcv *ListRangeResponse) Init(buf []byte, i flatbuffers.UOffsetT) {
	rcv._tab.Bytes = buf
	rcv._tab.Pos = i
}

func (rcv *ListRangeResponse) Table() flatbuffers.Table {
	return rcv._tab
}

func (rcv *ListRangeResponse) Status(obj *Status) *Status {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(4))
	if o != 0 {
		x := rcv._tab.Indirect(o + rcv._tab.Pos)
		if obj == nil {
			obj = new(Status)
		}
		obj.Init(rcv._tab.Bytes, x)
		return obj
	}
	return nil
}

func (rcv *ListRangeResponse) ThrottleTimeMs() int32 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(6))
	if o != 0 {
		return rcv._tab.GetInt32(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *ListRangeResponse) MutateThrottleTimeMs(n int32) bool {
	return rcv._tab.MutateInt32Slot(6, n)
}

func (rcv *ListRangeResponse) Ranges(obj *Range, j int) bool {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(8))
	if o != 0 {
		x := rcv._tab.Vector(o)
		x += flatbuffers.UOffsetT(j) * 4
		x = rcv._tab.Indirect(x)
		obj.Init(rcv._tab.Bytes, x)
		return true
	}
	return false
}

func (rcv *ListRangeResponse) RangesLength() int {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(8))
	if o != 0 {
		return rcv._tab.VectorLen(o)
	}
	return 0
}

func ListRangeResponseStart(builder *flatbuffers.Builder) {
	builder.StartObject(3)
}
func ListRangeResponseAddStatus(builder *flatbuffers.Builder, status flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(0, flatbuffers.UOffsetT(status), 0)
}
func ListRangeResponseAddThrottleTimeMs(builder *flatbuffers.Builder, throttleTimeMs int32) {
	builder.PrependInt32Slot(1, throttleTimeMs, 0)
}
func ListRangeResponseAddRanges(builder *flatbuffers.Builder, ranges flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(2, flatbuffers.UOffsetT(ranges), 0)
}
func ListRangeResponseStartRangesVector(builder *flatbuffers.Builder, numElems int) flatbuffers.UOffsetT {
	return builder.StartVector(4, numElems, 4)
}
func ListRangeResponseEnd(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	return builder.EndObject()
}
