package protocol

import (
	"context"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/codec/format"
	"github.com/AutoMQ/placement-manager/pkg/sbp/codec/operation"
	"github.com/AutoMQ/placement-manager/pkg/util/fbutil"
)

// request is an SBP request
type request interface {
	// Timeout returns the timeout of the request in milliseconds.
	// It returns 0 if the request doesn't have a timeout.
	Timeout() int32

	// SetContext sets the context of the request.
	// The provided ctx must be non-nil.
	SetContext(ctx context.Context)

	// Context returns the context of the request.
	// For outgoing client requests, the context controls cancellation.
	// For incoming server requests, the context is canceled when the client's connection closes.
	Context() context.Context
}

type InRequest interface {
	request
	unmarshaler
}

type OutRequest interface {
	request
	marshaller

	// Operation returns the operation of the request.
	Operation() operation.Operation
}

// baseRequest is a base implementation of Request
type baseRequest struct {
	ctx context.Context
}

func (req *baseRequest) Timeout() int32 {
	return 0
}

func (req *baseRequest) SetContext(ctx context.Context) {
	req.ctx = ctx
}

func (req *baseRequest) Context() context.Context {
	if req.ctx == nil {
		return context.Background()
	}
	return req.ctx
}

// EmptyRequest is an empty request, used for unrecognized requests
type EmptyRequest struct {
	baseRequest
	baseUnmarshaler
}

func (e EmptyRequest) Unmarshal(_ format.Format, _ []byte) error {
	_ = e.baseUnmarshaler
	return nil
}

// HeartbeatRequest is a request to operation.OpHeartbeat
type HeartbeatRequest struct {
	baseRequest
	baseMarshaller
	baseUnmarshaler

	rpcfb.HeartbeatRequestT
}

func (hr *HeartbeatRequest) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&hr.HeartbeatRequestT), nil
}

func (hr *HeartbeatRequest) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(hr, fmt)
}

func (hr *HeartbeatRequest) unmarshalFlatBuffer(data []byte) error {
	hr.HeartbeatRequestT = *rpcfb.GetRootAsHeartbeatRequest(data, 0).UnPack()
	return nil
}

func (hr *HeartbeatRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(hr, fmt, data)
}

func (hr *HeartbeatRequest) Operation() operation.Operation {
	return operation.Operation{Code: operation.OpHeartbeat}
}

// IDAllocationRequest is a request to operation.OpAllocateID
type IDAllocationRequest struct {
	baseRequest
	baseUnmarshaler

	rpcfb.IdAllocationRequestT
}

func (ia *IDAllocationRequest) unmarshalFlatBuffer(data []byte) error {
	ia.IdAllocationRequestT = *rpcfb.GetRootAsIdAllocationRequest(data, 0).UnPack()
	return nil
}

func (ia *IDAllocationRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(ia, fmt, data)
}

func (ia *IDAllocationRequest) Timeout() int32 {
	return ia.TimeoutMs
}

// ListRangeRequest is a request to operation.OpListRange
type ListRangeRequest struct {
	baseRequest
	baseUnmarshaler

	rpcfb.ListRangeRequestT
}

func (lr *ListRangeRequest) unmarshalFlatBuffer(data []byte) error {
	lr.ListRangeRequestT = *rpcfb.GetRootAsListRangeRequest(data, 0).UnPack()
	return nil
}

func (lr *ListRangeRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(lr, fmt, data)
}

func (lr *ListRangeRequest) Timeout() int32 {
	return lr.TimeoutMs
}

// SealRangeRequest is a request to operation.OpSealRange
type SealRangeRequest struct {
	baseRequest
	baseUnmarshaler

	rpcfb.SealRangeRequestT
}

func (sr *SealRangeRequest) unmarshalFlatBuffer(data []byte) error {
	sr.SealRangeRequestT = *rpcfb.GetRootAsSealRangeRequest(data, 0).UnPack()
	return nil
}

func (sr *SealRangeRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(sr, fmt, data)
}

func (sr *SealRangeRequest) Timeout() int32 {
	return sr.TimeoutMs
}

// CreateRangeRequest is a request to operation.OpCreateRange
type CreateRangeRequest struct {
	baseRequest
	baseUnmarshaler

	rpcfb.CreateRangeRequestT
}

func (cr *CreateRangeRequest) unmarshalFlatBuffer(data []byte) error {
	cr.CreateRangeRequestT = *rpcfb.GetRootAsCreateRangeRequest(data, 0).UnPack()
	return nil
}

func (cr *CreateRangeRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(cr, fmt, data)
}

func (cr *CreateRangeRequest) Timeout() int32 {
	return cr.TimeoutMs
}

// CreateStreamRequest is a request to operation.OpCreateStream
type CreateStreamRequest struct {
	baseRequest
	baseUnmarshaler

	rpcfb.CreateStreamRequestT
}

func (cs *CreateStreamRequest) unmarshalFlatBuffer(data []byte) error {
	cs.CreateStreamRequestT = *rpcfb.GetRootAsCreateStreamRequest(data, 0).UnPack()
	return nil
}

func (cs *CreateStreamRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(cs, fmt, data)
}

func (cs *CreateStreamRequest) Timeout() int32 {
	return cs.TimeoutMs
}

// DeleteStreamRequest is a request to operation.OpDeleteStream
type DeleteStreamRequest struct {
	baseRequest
	baseUnmarshaler

	rpcfb.DeleteStreamRequestT
}

func (ds *DeleteStreamRequest) unmarshalFlatBuffer(data []byte) error {
	ds.DeleteStreamRequestT = *rpcfb.GetRootAsDeleteStreamRequest(data, 0).UnPack()
	return nil
}

func (ds *DeleteStreamRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(ds, fmt, data)
}

func (ds *DeleteStreamRequest) Timeout() int32 {
	return ds.TimeoutMs
}

// UpdateStreamRequest is a request to operation.OpUpdateStream
type UpdateStreamRequest struct {
	baseRequest
	baseUnmarshaler

	rpcfb.UpdateStreamRequestT
}

func (us *UpdateStreamRequest) unmarshalFlatBuffer(data []byte) error {
	us.UpdateStreamRequestT = *rpcfb.GetRootAsUpdateStreamRequest(data, 0).UnPack()
	return nil
}

func (us *UpdateStreamRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(us, fmt, data)
}

func (us *UpdateStreamRequest) Timeout() int32 {
	return us.TimeoutMs
}

// DescribeStreamRequest is a request to operation.OpDescribeStream
type DescribeStreamRequest struct {
	baseRequest
	baseUnmarshaler

	rpcfb.DescribeStreamRequestT
}

func (ds *DescribeStreamRequest) unmarshalFlatBuffer(data []byte) error {
	ds.DescribeStreamRequestT = *rpcfb.GetRootAsDescribeStreamRequest(data, 0).UnPack()
	return nil
}

func (ds *DescribeStreamRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(ds, fmt, data)
}

func (ds *DescribeStreamRequest) Timeout() int32 {
	return ds.TimeoutMs
}

// DescribePMClusterRequest is a request to operation.OpDescribePMCluster
type DescribePMClusterRequest struct {
	baseRequest
	baseUnmarshaler

	rpcfb.DescribePlacementManagerClusterRequestT
}

func (dpm *DescribePMClusterRequest) unmarshalFlatBuffer(data []byte) error {
	dpm.DescribePlacementManagerClusterRequestT = *rpcfb.GetRootAsDescribePlacementManagerClusterRequest(data, 0).UnPack()
	return nil
}

func (dpm *DescribePMClusterRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(dpm, fmt, data)
}

func (dpm *DescribePMClusterRequest) Timeout() int32 {
	return dpm.TimeoutMs
}
