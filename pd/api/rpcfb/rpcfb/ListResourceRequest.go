// Code generated by the FlatBuffers compiler. DO NOT EDIT.

package rpcfb

import (
	flatbuffers "github.com/google/flatbuffers/go"
)

type ListResourceRequestT struct {
	TimeoutMs int32 `json:"timeout_ms"`
	ResourceType []ResourceType `json:"resource_type"`
	Limit int32 `json:"limit"`
	Continuation []byte `json:"continuation"`
}

func (t *ListResourceRequestT) Pack(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	if t == nil { return 0 }
	resourceTypeOffset := flatbuffers.UOffsetT(0)
	if t.ResourceType != nil {
		resourceTypeLength := len(t.ResourceType)
		ListResourceRequestStartResourceTypeVector(builder, resourceTypeLength)
		for j := resourceTypeLength - 1; j >= 0; j-- {
			builder.PrependInt8(int8(t.ResourceType[j]))
		}
		resourceTypeOffset = builder.EndVector(resourceTypeLength)
	}
	continuationOffset := flatbuffers.UOffsetT(0)
	if t.Continuation != nil {
		continuationOffset = builder.CreateByteString(t.Continuation)
	}
	ListResourceRequestStart(builder)
	ListResourceRequestAddTimeoutMs(builder, t.TimeoutMs)
	ListResourceRequestAddResourceType(builder, resourceTypeOffset)
	ListResourceRequestAddLimit(builder, t.Limit)
	ListResourceRequestAddContinuation(builder, continuationOffset)
	return ListResourceRequestEnd(builder)
}

func (rcv *ListResourceRequest) UnPackTo(t *ListResourceRequestT) {
	t.TimeoutMs = rcv.TimeoutMs()
	resourceTypeLength := rcv.ResourceTypeLength()
	t.ResourceType = make([]ResourceType, resourceTypeLength)
	for j := 0; j < resourceTypeLength; j++ {
		t.ResourceType[j] = rcv.ResourceType(j)
	}
	t.Limit = rcv.Limit()
	t.Continuation = rcv.ContinuationBytes()
}

func (rcv *ListResourceRequest) UnPack() *ListResourceRequestT {
	if rcv == nil { return nil }
	t := &ListResourceRequestT{}
	rcv.UnPackTo(t)
	return t
}

type ListResourceRequest struct {
	_tab flatbuffers.Table
}

func GetRootAsListResourceRequest(buf []byte, offset flatbuffers.UOffsetT) *ListResourceRequest {
	n := flatbuffers.GetUOffsetT(buf[offset:])
	x := &ListResourceRequest{}
	x.Init(buf, n+offset)
	return x
}

func GetSizePrefixedRootAsListResourceRequest(buf []byte, offset flatbuffers.UOffsetT) *ListResourceRequest {
	n := flatbuffers.GetUOffsetT(buf[offset+flatbuffers.SizeUint32:])
	x := &ListResourceRequest{}
	x.Init(buf, n+offset+flatbuffers.SizeUint32)
	return x
}

func (rcv *ListResourceRequest) Init(buf []byte, i flatbuffers.UOffsetT) {
	rcv._tab.Bytes = buf
	rcv._tab.Pos = i
}

func (rcv *ListResourceRequest) Table() flatbuffers.Table {
	return rcv._tab
}

func (rcv *ListResourceRequest) TimeoutMs() int32 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(4))
	if o != 0 {
		return rcv._tab.GetInt32(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *ListResourceRequest) MutateTimeoutMs(n int32) bool {
	return rcv._tab.MutateInt32Slot(4, n)
}

func (rcv *ListResourceRequest) ResourceType(j int) ResourceType {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(6))
	if o != 0 {
		a := rcv._tab.Vector(o)
		return ResourceType(rcv._tab.GetInt8(a + flatbuffers.UOffsetT(j*1)))
	}
	return 0
}

func (rcv *ListResourceRequest) ResourceTypeLength() int {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(6))
	if o != 0 {
		return rcv._tab.VectorLen(o)
	}
	return 0
}

func (rcv *ListResourceRequest) MutateResourceType(j int, n ResourceType) bool {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(6))
	if o != 0 {
		a := rcv._tab.Vector(o)
		return rcv._tab.MutateInt8(a+flatbuffers.UOffsetT(j*1), int8(n))
	}
	return false
}

func (rcv *ListResourceRequest) Limit() int32 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(8))
	if o != 0 {
		return rcv._tab.GetInt32(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *ListResourceRequest) MutateLimit(n int32) bool {
	return rcv._tab.MutateInt32Slot(8, n)
}

func (rcv *ListResourceRequest) Continuation(j int) byte {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(10))
	if o != 0 {
		a := rcv._tab.Vector(o)
		return rcv._tab.GetByte(a + flatbuffers.UOffsetT(j*1))
	}
	return 0
}

func (rcv *ListResourceRequest) ContinuationLength() int {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(10))
	if o != 0 {
		return rcv._tab.VectorLen(o)
	}
	return 0
}

func (rcv *ListResourceRequest) ContinuationBytes() []byte {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(10))
	if o != 0 {
		return rcv._tab.ByteVector(o + rcv._tab.Pos)
	}
	return nil
}

func (rcv *ListResourceRequest) MutateContinuation(j int, n byte) bool {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(10))
	if o != 0 {
		a := rcv._tab.Vector(o)
		return rcv._tab.MutateByte(a+flatbuffers.UOffsetT(j*1), n)
	}
	return false
}

func ListResourceRequestStart(builder *flatbuffers.Builder) {
	builder.StartObject(4)
}
func ListResourceRequestAddTimeoutMs(builder *flatbuffers.Builder, timeoutMs int32) {
	builder.PrependInt32Slot(0, timeoutMs, 0)
}
func ListResourceRequestAddResourceType(builder *flatbuffers.Builder, resourceType flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(1, flatbuffers.UOffsetT(resourceType), 0)
}
func ListResourceRequestStartResourceTypeVector(builder *flatbuffers.Builder, numElems int) flatbuffers.UOffsetT {
	return builder.StartVector(1, numElems, 1)
}
func ListResourceRequestAddLimit(builder *flatbuffers.Builder, limit int32) {
	builder.PrependInt32Slot(2, limit, 0)
}
func ListResourceRequestAddContinuation(builder *flatbuffers.Builder, continuation flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(3, flatbuffers.UOffsetT(continuation), 0)
}
func ListResourceRequestStartContinuationVector(builder *flatbuffers.Builder, numElems int) flatbuffers.UOffsetT {
	return builder.StartVector(1, numElems, 1)
}
func ListResourceRequestEnd(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	return builder.EndObject()
}
