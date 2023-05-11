// Code generated by the FlatBuffers compiler. DO NOT EDIT.

package rpcfb

import (
	flatbuffers "github.com/google/flatbuffers/go"
)

type SealRangeResponseT struct {
	Status *StatusT `json:"status"`
	Range *RangeT `json:"range"`
	ThrottleTimeMs int32 `json:"throttle_time_ms"`
}

func (t *SealRangeResponseT) Pack(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	if t == nil { return 0 }
	statusOffset := t.Status.Pack(builder)
	range_Offset := t.Range.Pack(builder)
	SealRangeResponseStart(builder)
	SealRangeResponseAddStatus(builder, statusOffset)
	SealRangeResponseAddRange(builder, range_Offset)
	SealRangeResponseAddThrottleTimeMs(builder, t.ThrottleTimeMs)
	return SealRangeResponseEnd(builder)
}

func (rcv *SealRangeResponse) UnPackTo(t *SealRangeResponseT) {
	t.Status = rcv.Status(nil).UnPack()
	t.Range = rcv.Range(nil).UnPack()
	t.ThrottleTimeMs = rcv.ThrottleTimeMs()
}

func (rcv *SealRangeResponse) UnPack() *SealRangeResponseT {
	if rcv == nil { return nil }
	t := &SealRangeResponseT{}
	rcv.UnPackTo(t)
	return t
}

type SealRangeResponse struct {
	_tab flatbuffers.Table
}

func GetRootAsSealRangeResponse(buf []byte, offset flatbuffers.UOffsetT) *SealRangeResponse {
	n := flatbuffers.GetUOffsetT(buf[offset:])
	x := &SealRangeResponse{}
	x.Init(buf, n+offset)
	return x
}

func GetSizePrefixedRootAsSealRangeResponse(buf []byte, offset flatbuffers.UOffsetT) *SealRangeResponse {
	n := flatbuffers.GetUOffsetT(buf[offset+flatbuffers.SizeUint32:])
	x := &SealRangeResponse{}
	x.Init(buf, n+offset+flatbuffers.SizeUint32)
	return x
}

func (rcv *SealRangeResponse) Init(buf []byte, i flatbuffers.UOffsetT) {
	rcv._tab.Bytes = buf
	rcv._tab.Pos = i
}

func (rcv *SealRangeResponse) Table() flatbuffers.Table {
	return rcv._tab
}

func (rcv *SealRangeResponse) Status(obj *Status) *Status {
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

func (rcv *SealRangeResponse) Range(obj *Range) *Range {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(6))
	if o != 0 {
		x := rcv._tab.Indirect(o + rcv._tab.Pos)
		if obj == nil {
			obj = new(Range)
		}
		obj.Init(rcv._tab.Bytes, x)
		return obj
	}
	return nil
}

func (rcv *SealRangeResponse) ThrottleTimeMs() int32 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(8))
	if o != 0 {
		return rcv._tab.GetInt32(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *SealRangeResponse) MutateThrottleTimeMs(n int32) bool {
	return rcv._tab.MutateInt32Slot(8, n)
}

func SealRangeResponseStart(builder *flatbuffers.Builder) {
	builder.StartObject(3)
}
func SealRangeResponseAddStatus(builder *flatbuffers.Builder, status flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(0, flatbuffers.UOffsetT(status), 0)
}
func SealRangeResponseAddRange(builder *flatbuffers.Builder, range_ flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(1, flatbuffers.UOffsetT(range_), 0)
}
func SealRangeResponseAddThrottleTimeMs(builder *flatbuffers.Builder, throttleTimeMs int32) {
	builder.PrependInt32Slot(2, throttleTimeMs, 0)
}
func SealRangeResponseEnd(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	return builder.EndObject()
}
