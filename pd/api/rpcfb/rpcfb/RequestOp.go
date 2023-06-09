// Code generated by the FlatBuffers compiler. DO NOT EDIT.

package rpcfb

import (
	flatbuffers "github.com/google/flatbuffers/go"
)

type RequestOpT struct {
	Type RequestOpType `json:"type"`
	RangeRequest *RangeRequestT `json:"range_request"`
	PutRequest *PutRequestT `json:"put_request"`
	DeleteRangeRequest *DeleteRangeRequestT `json:"delete_range_request"`
	TxnRequest *TxnRequestT `json:"txn_request"`
}

func (t *RequestOpT) Pack(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	if t == nil { return 0 }
	rangeRequestOffset := t.RangeRequest.Pack(builder)
	putRequestOffset := t.PutRequest.Pack(builder)
	deleteRangeRequestOffset := t.DeleteRangeRequest.Pack(builder)
	txnRequestOffset := t.TxnRequest.Pack(builder)
	RequestOpStart(builder)
	RequestOpAddType(builder, t.Type)
	RequestOpAddRangeRequest(builder, rangeRequestOffset)
	RequestOpAddPutRequest(builder, putRequestOffset)
	RequestOpAddDeleteRangeRequest(builder, deleteRangeRequestOffset)
	RequestOpAddTxnRequest(builder, txnRequestOffset)
	return RequestOpEnd(builder)
}

func (rcv *RequestOp) UnPackTo(t *RequestOpT) {
	t.Type = rcv.Type()
	t.RangeRequest = rcv.RangeRequest(nil).UnPack()
	t.PutRequest = rcv.PutRequest(nil).UnPack()
	t.DeleteRangeRequest = rcv.DeleteRangeRequest(nil).UnPack()
	t.TxnRequest = rcv.TxnRequest(nil).UnPack()
}

func (rcv *RequestOp) UnPack() *RequestOpT {
	if rcv == nil { return nil }
	t := &RequestOpT{}
	rcv.UnPackTo(t)
	return t
}

type RequestOp struct {
	_tab flatbuffers.Table
}

func GetRootAsRequestOp(buf []byte, offset flatbuffers.UOffsetT) *RequestOp {
	n := flatbuffers.GetUOffsetT(buf[offset:])
	x := &RequestOp{}
	x.Init(buf, n+offset)
	return x
}

func GetSizePrefixedRootAsRequestOp(buf []byte, offset flatbuffers.UOffsetT) *RequestOp {
	n := flatbuffers.GetUOffsetT(buf[offset+flatbuffers.SizeUint32:])
	x := &RequestOp{}
	x.Init(buf, n+offset+flatbuffers.SizeUint32)
	return x
}

func (rcv *RequestOp) Init(buf []byte, i flatbuffers.UOffsetT) {
	rcv._tab.Bytes = buf
	rcv._tab.Pos = i
}

func (rcv *RequestOp) Table() flatbuffers.Table {
	return rcv._tab
}

func (rcv *RequestOp) Type() RequestOpType {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(4))
	if o != 0 {
		return RequestOpType(rcv._tab.GetByte(o + rcv._tab.Pos))
	}
	return 0
}

func (rcv *RequestOp) MutateType(n RequestOpType) bool {
	return rcv._tab.MutateByteSlot(4, byte(n))
}

func (rcv *RequestOp) RangeRequest(obj *RangeRequest) *RangeRequest {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(6))
	if o != 0 {
		x := rcv._tab.Indirect(o + rcv._tab.Pos)
		if obj == nil {
			obj = new(RangeRequest)
		}
		obj.Init(rcv._tab.Bytes, x)
		return obj
	}
	return nil
}

func (rcv *RequestOp) PutRequest(obj *PutRequest) *PutRequest {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(8))
	if o != 0 {
		x := rcv._tab.Indirect(o + rcv._tab.Pos)
		if obj == nil {
			obj = new(PutRequest)
		}
		obj.Init(rcv._tab.Bytes, x)
		return obj
	}
	return nil
}

func (rcv *RequestOp) DeleteRangeRequest(obj *DeleteRangeRequest) *DeleteRangeRequest {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(10))
	if o != 0 {
		x := rcv._tab.Indirect(o + rcv._tab.Pos)
		if obj == nil {
			obj = new(DeleteRangeRequest)
		}
		obj.Init(rcv._tab.Bytes, x)
		return obj
	}
	return nil
}

func (rcv *RequestOp) TxnRequest(obj *TxnRequest) *TxnRequest {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(12))
	if o != 0 {
		x := rcv._tab.Indirect(o + rcv._tab.Pos)
		if obj == nil {
			obj = new(TxnRequest)
		}
		obj.Init(rcv._tab.Bytes, x)
		return obj
	}
	return nil
}

func RequestOpStart(builder *flatbuffers.Builder) {
	builder.StartObject(5)
}
func RequestOpAddType(builder *flatbuffers.Builder, type_ RequestOpType) {
	builder.PrependByteSlot(0, byte(type_), 0)
}
func RequestOpAddRangeRequest(builder *flatbuffers.Builder, rangeRequest flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(1, flatbuffers.UOffsetT(rangeRequest), 0)
}
func RequestOpAddPutRequest(builder *flatbuffers.Builder, putRequest flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(2, flatbuffers.UOffsetT(putRequest), 0)
}
func RequestOpAddDeleteRangeRequest(builder *flatbuffers.Builder, deleteRangeRequest flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(3, flatbuffers.UOffsetT(deleteRangeRequest), 0)
}
func RequestOpAddTxnRequest(builder *flatbuffers.Builder, txnRequest flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(4, flatbuffers.UOffsetT(txnRequest), 0)
}
func RequestOpEnd(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	return builder.EndObject()
}
