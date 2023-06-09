// Code generated by the FlatBuffers compiler. DO NOT EDIT.

package rpcfb

import (
	flatbuffers "github.com/google/flatbuffers/go"
)

type ListRangeRequestT struct {
	TimeoutMs int32 `json:"timeout_ms"`
	Criteria *ListRangeCriteriaT `json:"criteria"`
}

func (t *ListRangeRequestT) Pack(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	if t == nil { return 0 }
	criteriaOffset := t.Criteria.Pack(builder)
	ListRangeRequestStart(builder)
	ListRangeRequestAddTimeoutMs(builder, t.TimeoutMs)
	ListRangeRequestAddCriteria(builder, criteriaOffset)
	return ListRangeRequestEnd(builder)
}

func (rcv *ListRangeRequest) UnPackTo(t *ListRangeRequestT) {
	t.TimeoutMs = rcv.TimeoutMs()
	t.Criteria = rcv.Criteria(nil).UnPack()
}

func (rcv *ListRangeRequest) UnPack() *ListRangeRequestT {
	if rcv == nil { return nil }
	t := &ListRangeRequestT{}
	rcv.UnPackTo(t)
	return t
}

type ListRangeRequest struct {
	_tab flatbuffers.Table
}

func GetRootAsListRangeRequest(buf []byte, offset flatbuffers.UOffsetT) *ListRangeRequest {
	n := flatbuffers.GetUOffsetT(buf[offset:])
	x := &ListRangeRequest{}
	x.Init(buf, n+offset)
	return x
}

func GetSizePrefixedRootAsListRangeRequest(buf []byte, offset flatbuffers.UOffsetT) *ListRangeRequest {
	n := flatbuffers.GetUOffsetT(buf[offset+flatbuffers.SizeUint32:])
	x := &ListRangeRequest{}
	x.Init(buf, n+offset+flatbuffers.SizeUint32)
	return x
}

func (rcv *ListRangeRequest) Init(buf []byte, i flatbuffers.UOffsetT) {
	rcv._tab.Bytes = buf
	rcv._tab.Pos = i
}

func (rcv *ListRangeRequest) Table() flatbuffers.Table {
	return rcv._tab
}

func (rcv *ListRangeRequest) TimeoutMs() int32 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(4))
	if o != 0 {
		return rcv._tab.GetInt32(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *ListRangeRequest) MutateTimeoutMs(n int32) bool {
	return rcv._tab.MutateInt32Slot(4, n)
}

func (rcv *ListRangeRequest) Criteria(obj *ListRangeCriteria) *ListRangeCriteria {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(6))
	if o != 0 {
		x := rcv._tab.Indirect(o + rcv._tab.Pos)
		if obj == nil {
			obj = new(ListRangeCriteria)
		}
		obj.Init(rcv._tab.Bytes, x)
		return obj
	}
	return nil
}

func ListRangeRequestStart(builder *flatbuffers.Builder) {
	builder.StartObject(2)
}
func ListRangeRequestAddTimeoutMs(builder *flatbuffers.Builder, timeoutMs int32) {
	builder.PrependInt32Slot(0, timeoutMs, 0)
}
func ListRangeRequestAddCriteria(builder *flatbuffers.Builder, criteria flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(1, flatbuffers.UOffsetT(criteria), 0)
}
func ListRangeRequestEnd(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	return builder.EndObject()
}
