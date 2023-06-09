// Code generated by the FlatBuffers compiler. DO NOT EDIT.

package rpcfb

import (
	flatbuffers "github.com/google/flatbuffers/go"
)

type DeleteRangeResponseT struct {
	Status *StatusT `json:"status"`
	Deleted int64 `json:"deleted"`
}

func (t *DeleteRangeResponseT) Pack(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	if t == nil { return 0 }
	statusOffset := t.Status.Pack(builder)
	DeleteRangeResponseStart(builder)
	DeleteRangeResponseAddStatus(builder, statusOffset)
	DeleteRangeResponseAddDeleted(builder, t.Deleted)
	return DeleteRangeResponseEnd(builder)
}

func (rcv *DeleteRangeResponse) UnPackTo(t *DeleteRangeResponseT) {
	t.Status = rcv.Status(nil).UnPack()
	t.Deleted = rcv.Deleted()
}

func (rcv *DeleteRangeResponse) UnPack() *DeleteRangeResponseT {
	if rcv == nil { return nil }
	t := &DeleteRangeResponseT{}
	rcv.UnPackTo(t)
	return t
}

type DeleteRangeResponse struct {
	_tab flatbuffers.Table
}

func GetRootAsDeleteRangeResponse(buf []byte, offset flatbuffers.UOffsetT) *DeleteRangeResponse {
	n := flatbuffers.GetUOffsetT(buf[offset:])
	x := &DeleteRangeResponse{}
	x.Init(buf, n+offset)
	return x
}

func GetSizePrefixedRootAsDeleteRangeResponse(buf []byte, offset flatbuffers.UOffsetT) *DeleteRangeResponse {
	n := flatbuffers.GetUOffsetT(buf[offset+flatbuffers.SizeUint32:])
	x := &DeleteRangeResponse{}
	x.Init(buf, n+offset+flatbuffers.SizeUint32)
	return x
}

func (rcv *DeleteRangeResponse) Init(buf []byte, i flatbuffers.UOffsetT) {
	rcv._tab.Bytes = buf
	rcv._tab.Pos = i
}

func (rcv *DeleteRangeResponse) Table() flatbuffers.Table {
	return rcv._tab
}

func (rcv *DeleteRangeResponse) Status(obj *Status) *Status {
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

func (rcv *DeleteRangeResponse) Deleted() int64 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(6))
	if o != 0 {
		return rcv._tab.GetInt64(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *DeleteRangeResponse) MutateDeleted(n int64) bool {
	return rcv._tab.MutateInt64Slot(6, n)
}

func DeleteRangeResponseStart(builder *flatbuffers.Builder) {
	builder.StartObject(2)
}
func DeleteRangeResponseAddStatus(builder *flatbuffers.Builder, status flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(0, flatbuffers.UOffsetT(status), 0)
}
func DeleteRangeResponseAddDeleted(builder *flatbuffers.Builder, deleted int64) {
	builder.PrependInt64Slot(1, deleted, 0)
}
func DeleteRangeResponseEnd(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	return builder.EndObject()
}
