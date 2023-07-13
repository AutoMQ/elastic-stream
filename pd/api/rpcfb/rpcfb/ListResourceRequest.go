// Code generated by the FlatBuffers compiler. DO NOT EDIT.

package rpcfb

import (
	flatbuffers "github.com/google/flatbuffers/go"
)

type ListResourceRequestT struct {
	TimeoutMs int32 `json:"timeout_ms"`
	ResourceType []ResourceType `json:"resource_type"`
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
	ListResourceRequestStart(builder)
	ListResourceRequestAddTimeoutMs(builder, t.TimeoutMs)
	ListResourceRequestAddResourceType(builder, resourceTypeOffset)
	return ListResourceRequestEnd(builder)
}

func (rcv *ListResourceRequest) UnPackTo(t *ListResourceRequestT) {
	t.TimeoutMs = rcv.TimeoutMs()
	resourceTypeLength := rcv.ResourceTypeLength()
	t.ResourceType = make([]ResourceType, resourceTypeLength)
	for j := 0; j < resourceTypeLength; j++ {
		t.ResourceType[j] = rcv.ResourceType(j)
	}
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

func ListResourceRequestStart(builder *flatbuffers.Builder) {
	builder.StartObject(2)
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
func ListResourceRequestEnd(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	return builder.EndObject()
}
