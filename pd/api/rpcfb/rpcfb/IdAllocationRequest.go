// Code generated by the FlatBuffers compiler. DO NOT EDIT.

package rpcfb

import (
	flatbuffers "github.com/google/flatbuffers/go"
)

type IdAllocationRequestT struct {
	TimeoutMs int32 `json:"timeout_ms"`
	Host string `json:"host"`
}

func (t *IdAllocationRequestT) Pack(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	if t == nil { return 0 }
	hostOffset := flatbuffers.UOffsetT(0)
	if t.Host != "" {
		hostOffset = builder.CreateString(t.Host)
	}
	IdAllocationRequestStart(builder)
	IdAllocationRequestAddTimeoutMs(builder, t.TimeoutMs)
	IdAllocationRequestAddHost(builder, hostOffset)
	return IdAllocationRequestEnd(builder)
}

func (rcv *IdAllocationRequest) UnPackTo(t *IdAllocationRequestT) {
	t.TimeoutMs = rcv.TimeoutMs()
	t.Host = string(rcv.Host())
}

func (rcv *IdAllocationRequest) UnPack() *IdAllocationRequestT {
	if rcv == nil { return nil }
	t := &IdAllocationRequestT{}
	rcv.UnPackTo(t)
	return t
}

type IdAllocationRequest struct {
	_tab flatbuffers.Table
}

func GetRootAsIdAllocationRequest(buf []byte, offset flatbuffers.UOffsetT) *IdAllocationRequest {
	n := flatbuffers.GetUOffsetT(buf[offset:])
	x := &IdAllocationRequest{}
	x.Init(buf, n+offset)
	return x
}

func GetSizePrefixedRootAsIdAllocationRequest(buf []byte, offset flatbuffers.UOffsetT) *IdAllocationRequest {
	n := flatbuffers.GetUOffsetT(buf[offset+flatbuffers.SizeUint32:])
	x := &IdAllocationRequest{}
	x.Init(buf, n+offset+flatbuffers.SizeUint32)
	return x
}

func (rcv *IdAllocationRequest) Init(buf []byte, i flatbuffers.UOffsetT) {
	rcv._tab.Bytes = buf
	rcv._tab.Pos = i
}

func (rcv *IdAllocationRequest) Table() flatbuffers.Table {
	return rcv._tab
}

func (rcv *IdAllocationRequest) TimeoutMs() int32 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(4))
	if o != 0 {
		return rcv._tab.GetInt32(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *IdAllocationRequest) MutateTimeoutMs(n int32) bool {
	return rcv._tab.MutateInt32Slot(4, n)
}

func (rcv *IdAllocationRequest) Host() []byte {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(6))
	if o != 0 {
		return rcv._tab.ByteVector(o + rcv._tab.Pos)
	}
	return nil
}

func IdAllocationRequestStart(builder *flatbuffers.Builder) {
	builder.StartObject(2)
}
func IdAllocationRequestAddTimeoutMs(builder *flatbuffers.Builder, timeoutMs int32) {
	builder.PrependInt32Slot(0, timeoutMs, 0)
}
func IdAllocationRequestAddHost(builder *flatbuffers.Builder, host flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(1, flatbuffers.UOffsetT(host), 0)
}
func IdAllocationRequestEnd(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	return builder.EndObject()
}
