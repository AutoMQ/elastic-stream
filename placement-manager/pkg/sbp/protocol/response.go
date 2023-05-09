package protocol

import (
	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/codec/format"
	"github.com/AutoMQ/placement-manager/pkg/util/fbutil"
)

// response is an SBP response
type response interface{}

type InResponse interface {
	response
	unmarshaler

	// ThrottleTime returns the time in milliseconds to throttle the client.
	// It returns 0 if the response doesn't have a throttle time.
	ThrottleTime() int32
}

type noThrottleResponse struct{}

func (n noThrottleResponse) ThrottleTime() int32 {
	return 0
}

type OutResponse interface {
	response
	marshaller

	// Error sets the error status of the response.
	Error(status *rpcfb.StatusT)

	// OK sets the status of the response to rpcfb.ErrorCodeOK.
	OK()

	// IsEnd returns true if the response is the last response of a request.
	IsEnd() bool
}

// singleResponse represents a response that corresponds to a single request.
// It is used when a request is expected to have only one response.
type singleResponse struct{}

func (s singleResponse) IsEnd() bool {
	return true
}

// SystemErrorResponse is used to return the error code and error message if the system error flag of sbp is set.
type SystemErrorResponse struct {
	baseMarshaller
	baseUnmarshaler
	noThrottleResponse
	singleResponse

	rpcfb.SystemErrorResponseT
}

func (se *SystemErrorResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&se.SystemErrorResponseT), nil
}

func (se *SystemErrorResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(se, fmt)
}

func (se *SystemErrorResponse) unmarshalFlatBuffer(data []byte) error {
	se.SystemErrorResponseT = *rpcfb.GetRootAsSystemErrorResponse(data, 0).UnPack()
	return nil
}

func (se *SystemErrorResponse) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(se, fmt, data)
}

func (se *SystemErrorResponse) Error(status *rpcfb.StatusT) {
	se.Status = status
}

func (se *SystemErrorResponse) OK() {
	se.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// HeartbeatResponse is a response to operation.OpHeartbeat
type HeartbeatResponse struct {
	baseMarshaller
	baseUnmarshaler
	noThrottleResponse
	singleResponse

	rpcfb.HeartbeatResponseT
}

func (hr *HeartbeatResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&hr.HeartbeatResponseT), nil
}

func (hr *HeartbeatResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(hr, fmt)
}

func (hr *HeartbeatResponse) unmarshalFlatBuffer(data []byte) error {
	hr.HeartbeatResponseT = *rpcfb.GetRootAsHeartbeatResponse(data, 0).UnPack()
	return nil
}

func (hr *HeartbeatResponse) Unmarshal(fmt format.Format, data []byte) error {
	return unmarshal(hr, fmt, data)
}

func (hr *HeartbeatResponse) Error(status *rpcfb.StatusT) {
	hr.Status = status
}

func (hr *HeartbeatResponse) OK() {
	hr.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// IDAllocationResponse is a response to operation.OpAllocateID
type IDAllocationResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.IdAllocationResponseT
}

func (ia *IDAllocationResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&ia.IdAllocationResponseT), nil
}

func (ia *IDAllocationResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(ia, fmt)
}

func (ia *IDAllocationResponse) Error(status *rpcfb.StatusT) {
	ia.Status = status
}

func (ia *IDAllocationResponse) OK() {
	ia.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// ListRangesResponse is a response to operation.OpListRanges
type ListRangesResponse struct {
	baseMarshaller

	rpcfb.ListRangesResponseT

	// HasNext indicates whether there are more responses after this one.
	HasNext bool
}

func (lr *ListRangesResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&lr.ListRangesResponseT), nil
}

func (lr *ListRangesResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(lr, fmt)
}

func (lr *ListRangesResponse) Error(status *rpcfb.StatusT) {
	lr.Status = status
}

func (lr *ListRangesResponse) IsEnd() bool {
	return !lr.HasNext
}

func (lr *ListRangesResponse) OK() {
	lr.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// SealRangesResponse is a response to operation.OpSealRanges
type SealRangesResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.SealRangesResponseT
}

func (sr *SealRangesResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&sr.SealRangesResponseT), nil
}

func (sr *SealRangesResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(sr, fmt)
}

func (sr *SealRangesResponse) Error(status *rpcfb.StatusT) {
	sr.Status = status
}

func (sr *SealRangesResponse) OK() {
	sr.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// CreateStreamsResponse is a response to operation.OpCreateStreams
type CreateStreamsResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.CreateStreamsResponseT
}

func (cs *CreateStreamsResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&cs.CreateStreamsResponseT), nil
}

func (cs *CreateStreamsResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(cs, fmt)
}

func (cs *CreateStreamsResponse) Error(status *rpcfb.StatusT) {
	cs.Status = status
}

func (cs *CreateStreamsResponse) OK() {
	cs.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// DeleteStreamsResponse is a response to operation.OpDeleteStreams
type DeleteStreamsResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.DeleteStreamsResponseT
}

func (ds *DeleteStreamsResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&ds.DeleteStreamsResponseT), nil
}

func (ds *DeleteStreamsResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(ds, fmt)
}

func (ds *DeleteStreamsResponse) Error(status *rpcfb.StatusT) {
	ds.Status = status
}

func (ds *DeleteStreamsResponse) OK() {
	ds.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// UpdateStreamsResponse is a response to operation.OpUpdateStreams
type UpdateStreamsResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.UpdateStreamsResponseT
}

func (us *UpdateStreamsResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&us.UpdateStreamsResponseT), nil
}

func (us *UpdateStreamsResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(us, fmt)
}

func (us *UpdateStreamsResponse) Error(status *rpcfb.StatusT) {
	us.Status = status
}

func (us *UpdateStreamsResponse) OK() {
	us.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// DescribeStreamsResponse is a response to operation.OpDescribeStreams
type DescribeStreamsResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.DescribeStreamsResponseT
}

func (ds *DescribeStreamsResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&ds.DescribeStreamsResponseT), nil
}

func (ds *DescribeStreamsResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(ds, fmt)
}

func (ds *DescribeStreamsResponse) Error(status *rpcfb.StatusT) {
	ds.Status = status
}

func (ds *DescribeStreamsResponse) OK() {
	ds.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// DescribePMClusterResponse is a response to operation.OpDescribePMCluster
type DescribePMClusterResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.DescribePlacementManagerClusterResponseT
}

func (dpm *DescribePMClusterResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&dpm.DescribePlacementManagerClusterResponseT), nil
}

func (dpm *DescribePMClusterResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(dpm, fmt)
}

func (dpm *DescribePMClusterResponse) Error(status *rpcfb.StatusT) {
	dpm.Status = status
}

func (dpm *DescribePMClusterResponse) OK() {
	dpm.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}
