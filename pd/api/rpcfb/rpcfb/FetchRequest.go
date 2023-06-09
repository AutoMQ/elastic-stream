// Code generated by the FlatBuffers compiler. DO NOT EDIT.

package rpcfb

import (
	flatbuffers "github.com/google/flatbuffers/go"
)

type FetchRequestT struct {
	MaxWaitMs int32 `json:"max_wait_ms"`
	Range *RangeT `json:"range"`
	Offset int64 `json:"offset"`
	Limit int64 `json:"limit"`
	MinBytes int32 `json:"min_bytes"`
	MaxBytes int32 `json:"max_bytes"`
}

func (t *FetchRequestT) Pack(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	if t == nil { return 0 }
	range_Offset := t.Range.Pack(builder)
	FetchRequestStart(builder)
	FetchRequestAddMaxWaitMs(builder, t.MaxWaitMs)
	FetchRequestAddRange(builder, range_Offset)
	FetchRequestAddOffset(builder, t.Offset)
	FetchRequestAddLimit(builder, t.Limit)
	FetchRequestAddMinBytes(builder, t.MinBytes)
	FetchRequestAddMaxBytes(builder, t.MaxBytes)
	return FetchRequestEnd(builder)
}

func (rcv *FetchRequest) UnPackTo(t *FetchRequestT) {
	t.MaxWaitMs = rcv.MaxWaitMs()
	t.Range = rcv.Range(nil).UnPack()
	t.Offset = rcv.Offset()
	t.Limit = rcv.Limit()
	t.MinBytes = rcv.MinBytes()
	t.MaxBytes = rcv.MaxBytes()
}

func (rcv *FetchRequest) UnPack() *FetchRequestT {
	if rcv == nil { return nil }
	t := &FetchRequestT{}
	rcv.UnPackTo(t)
	return t
}

type FetchRequest struct {
	_tab flatbuffers.Table
}

func GetRootAsFetchRequest(buf []byte, offset flatbuffers.UOffsetT) *FetchRequest {
	n := flatbuffers.GetUOffsetT(buf[offset:])
	x := &FetchRequest{}
	x.Init(buf, n+offset)
	return x
}

func GetSizePrefixedRootAsFetchRequest(buf []byte, offset flatbuffers.UOffsetT) *FetchRequest {
	n := flatbuffers.GetUOffsetT(buf[offset+flatbuffers.SizeUint32:])
	x := &FetchRequest{}
	x.Init(buf, n+offset+flatbuffers.SizeUint32)
	return x
}

func (rcv *FetchRequest) Init(buf []byte, i flatbuffers.UOffsetT) {
	rcv._tab.Bytes = buf
	rcv._tab.Pos = i
}

func (rcv *FetchRequest) Table() flatbuffers.Table {
	return rcv._tab
}

func (rcv *FetchRequest) MaxWaitMs() int32 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(4))
	if o != 0 {
		return rcv._tab.GetInt32(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *FetchRequest) MutateMaxWaitMs(n int32) bool {
	return rcv._tab.MutateInt32Slot(4, n)
}

func (rcv *FetchRequest) Range(obj *Range) *Range {
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

func (rcv *FetchRequest) Offset() int64 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(8))
	if o != 0 {
		return rcv._tab.GetInt64(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *FetchRequest) MutateOffset(n int64) bool {
	return rcv._tab.MutateInt64Slot(8, n)
}

func (rcv *FetchRequest) Limit() int64 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(10))
	if o != 0 {
		return rcv._tab.GetInt64(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *FetchRequest) MutateLimit(n int64) bool {
	return rcv._tab.MutateInt64Slot(10, n)
}

func (rcv *FetchRequest) MinBytes() int32 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(12))
	if o != 0 {
		return rcv._tab.GetInt32(o + rcv._tab.Pos)
	}
	return -1
}

func (rcv *FetchRequest) MutateMinBytes(n int32) bool {
	return rcv._tab.MutateInt32Slot(12, n)
}

func (rcv *FetchRequest) MaxBytes() int32 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(14))
	if o != 0 {
		return rcv._tab.GetInt32(o + rcv._tab.Pos)
	}
	return -1
}

func (rcv *FetchRequest) MutateMaxBytes(n int32) bool {
	return rcv._tab.MutateInt32Slot(14, n)
}

func FetchRequestStart(builder *flatbuffers.Builder) {
	builder.StartObject(6)
}
func FetchRequestAddMaxWaitMs(builder *flatbuffers.Builder, maxWaitMs int32) {
	builder.PrependInt32Slot(0, maxWaitMs, 0)
}
func FetchRequestAddRange(builder *flatbuffers.Builder, range_ flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(1, flatbuffers.UOffsetT(range_), 0)
}
func FetchRequestAddOffset(builder *flatbuffers.Builder, offset int64) {
	builder.PrependInt64Slot(2, offset, 0)
}
func FetchRequestAddLimit(builder *flatbuffers.Builder, limit int64) {
	builder.PrependInt64Slot(3, limit, 0)
}
func FetchRequestAddMinBytes(builder *flatbuffers.Builder, minBytes int32) {
	builder.PrependInt32Slot(4, minBytes, -1)
}
func FetchRequestAddMaxBytes(builder *flatbuffers.Builder, maxBytes int32) {
	builder.PrependInt32Slot(5, maxBytes, -1)
}
func FetchRequestEnd(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	return builder.EndObject()
}
