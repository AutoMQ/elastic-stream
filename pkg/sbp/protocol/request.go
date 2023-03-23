package protocol

import (
	"context"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/codec/format"
	"github.com/AutoMQ/placement-manager/pkg/util/fbutil"
)

// Request is an SBP request
type Request interface {
	base

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

type HeartbeatRequest struct {
	baseRequest
	rpcfb.HeartbeatRequestT
}

func (hr *HeartbeatRequest) unmarshalFlatBuffer(data []byte) error {
	hr.HeartbeatRequestT = *rpcfb.GetRootAsHeartbeatRequest(data, 0).UnPack()
	return nil
}

func (hr *HeartbeatRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(hr, fmt, data)
}

// ListRangesRequest is a request to operation.OpListRanges
type ListRangesRequest struct {
	baseRequest
	rpcfb.ListRangesRequestT
}

func (lr *ListRangesRequest) unmarshalFlatBuffer(data []byte) error {
	lr.ListRangesRequestT = *rpcfb.GetRootAsListRangesRequest(data, 0).UnPack()
	return nil
}

func (lr *ListRangesRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(lr, fmt, data)
}

func (lr *ListRangesRequest) Timeout() int32 {
	return lr.TimeoutMs
}

// SealRangesRequest is a request to operation.OpSealRanges
type SealRangesRequest struct {
	baseRequest
	rpcfb.SealRangesRequestT
}

func (sr *SealRangesRequest) unmarshalFlatBuffer(data []byte) error {
	sr.SealRangesRequestT = *rpcfb.GetRootAsSealRangesRequest(data, 0).UnPack()
	return nil
}

func (sr *SealRangesRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(sr, fmt, data)
}

func (sr *SealRangesRequest) Timeout() int32 {
	return sr.TimeoutMs
}

func (sr *SealRangesRequest) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&sr.SealRangesRequestT), nil
}

func (sr *SealRangesRequest) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(sr, fmt)
}

// CreateStreamsRequest is a request to operation.OpCreateStreams
type CreateStreamsRequest struct {
	baseRequest
	rpcfb.CreateStreamsRequestT
}

func (cs *CreateStreamsRequest) unmarshalFlatBuffer(data []byte) error {
	cs.CreateStreamsRequestT = *rpcfb.GetRootAsCreateStreamsRequest(data, 0).UnPack()
	return nil
}

func (cs *CreateStreamsRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(cs, fmt, data)
}

func (cs *CreateStreamsRequest) Timeout() int32 {
	return cs.TimeoutMs
}

// DeleteStreamsRequest is a request to operation.OpDeleteStreams
type DeleteStreamsRequest struct {
	baseRequest
	rpcfb.DeleteStreamsRequestT
}

func (ds *DeleteStreamsRequest) unmarshalFlatBuffer(data []byte) error {
	ds.DeleteStreamsRequestT = *rpcfb.GetRootAsDeleteStreamsRequest(data, 0).UnPack()
	return nil
}

func (ds *DeleteStreamsRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(ds, fmt, data)
}

func (ds *DeleteStreamsRequest) Timeout() int32 {
	return ds.TimeoutMs
}

// UpdateStreamsRequest is a request to operation.OpUpdateStreams
type UpdateStreamsRequest struct {
	baseRequest
	rpcfb.UpdateStreamsRequestT
}

func (us *UpdateStreamsRequest) unmarshalFlatBuffer(data []byte) error {
	us.UpdateStreamsRequestT = *rpcfb.GetRootAsUpdateStreamsRequest(data, 0).UnPack()
	return nil
}

func (us *UpdateStreamsRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(us, fmt, data)
}

func (us *UpdateStreamsRequest) Timeout() int32 {
	return us.TimeoutMs
}

// DescribeStreamsRequest is a request to operation.OpDescribeStreams
type DescribeStreamsRequest struct {
	baseRequest
	rpcfb.DescribeStreamsRequestT
}

func (ds *DescribeStreamsRequest) unmarshalFlatBuffer(data []byte) error {
	ds.DescribeStreamsRequestT = *rpcfb.GetRootAsDescribeStreamsRequest(data, 0).UnPack()
	return nil
}

func (ds *DescribeStreamsRequest) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(ds, fmt, data)
}

func (ds *DescribeStreamsRequest) Timeout() int32 {
	return ds.TimeoutMs
}
