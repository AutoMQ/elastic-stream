package protocol

import (
	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/sbp/codec/format"
	"github.com/AutoMQ/pd/pkg/util/fbutil"
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

	rpcfb.SystemErrorT
}

func (se *SystemErrorResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&se.SystemErrorT), nil
}

func (se *SystemErrorResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(se, fmt)
}

func (se *SystemErrorResponse) unmarshalFlatBuffer(data []byte) error {
	se.SystemErrorT = *rpcfb.GetRootAsSystemError(data, 0).UnPack()
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

// ListRangeResponse is a response to operation.OpListRange
type ListRangeResponse struct {
	baseMarshaller

	rpcfb.ListRangeResponseT

	// HasNext indicates whether there are more responses after this one.
	HasNext bool
}

func (lr *ListRangeResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&lr.ListRangeResponseT), nil
}

func (lr *ListRangeResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(lr, fmt)
}

func (lr *ListRangeResponse) Error(status *rpcfb.StatusT) {
	lr.Status = status
}

func (lr *ListRangeResponse) IsEnd() bool {
	return !lr.HasNext
}

func (lr *ListRangeResponse) OK() {
	lr.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// SealRangeResponse is a response to operation.OpSealRange
type SealRangeResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.SealRangeResponseT
}

func (sr *SealRangeResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&sr.SealRangeResponseT), nil
}

func (sr *SealRangeResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(sr, fmt)
}

func (sr *SealRangeResponse) Error(status *rpcfb.StatusT) {
	sr.Status = status
}

func (sr *SealRangeResponse) OK() {
	sr.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// CreateRangeResponse is a response to operation.OpCreateRange
type CreateRangeResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.CreateRangeResponseT
}

func (cr *CreateRangeResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&cr.CreateRangeResponseT), nil
}

func (cr *CreateRangeResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(cr, fmt)
}

func (cr *CreateRangeResponse) Error(status *rpcfb.StatusT) {
	cr.Status = status
}

func (cr *CreateRangeResponse) OK() {
	cr.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// CreateStreamResponse is a response to operation.OpCreateStream
type CreateStreamResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.CreateStreamResponseT
}

func (cs *CreateStreamResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&cs.CreateStreamResponseT), nil
}

func (cs *CreateStreamResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(cs, fmt)
}

func (cs *CreateStreamResponse) Error(status *rpcfb.StatusT) {
	cs.Status = status
}

func (cs *CreateStreamResponse) OK() {
	cs.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// DeleteStreamResponse is a response to operation.OpDeleteStream
type DeleteStreamResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.DeleteStreamResponseT
}

func (ds *DeleteStreamResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&ds.DeleteStreamResponseT), nil
}

func (ds *DeleteStreamResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(ds, fmt)
}

func (ds *DeleteStreamResponse) Error(status *rpcfb.StatusT) {
	ds.Status = status
}

func (ds *DeleteStreamResponse) OK() {
	ds.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// UpdateStreamResponse is a response to operation.OpUpdateStream
type UpdateStreamResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.UpdateStreamResponseT
}

func (us *UpdateStreamResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&us.UpdateStreamResponseT), nil
}

func (us *UpdateStreamResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(us, fmt)
}

func (us *UpdateStreamResponse) Error(status *rpcfb.StatusT) {
	us.Status = status
}

func (us *UpdateStreamResponse) OK() {
	us.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// DescribeStreamResponse is a response to operation.OpDescribeStream
type DescribeStreamResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.DescribeStreamResponseT
}

func (ds *DescribeStreamResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&ds.DescribeStreamResponseT), nil
}

func (ds *DescribeStreamResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(ds, fmt)
}

func (ds *DescribeStreamResponse) Error(status *rpcfb.StatusT) {
	ds.Status = status
}

func (ds *DescribeStreamResponse) OK() {
	ds.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// ReportMetricsResponse is a response to operation.OpReportMetrics
type ReportMetricsResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.ReportMetricsResponseT
}

func (rm *ReportMetricsResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&rm.ReportMetricsResponseT), nil
}

func (rm *ReportMetricsResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(rm, fmt)
}

func (rm *ReportMetricsResponse) Error(status *rpcfb.StatusT) {
	rm.Status = status
}

func (rm *ReportMetricsResponse) OK() {
	rm.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}

// DescribePDClusterResponse is a response to operation.OpDescribePDCluster
type DescribePDClusterResponse struct {
	baseMarshaller
	singleResponse

	rpcfb.DescribePlacementDriverClusterResponseT
}

func (dpd *DescribePDClusterResponse) marshalFlatBuffer() ([]byte, error) {
	return fbutil.Marshal(&dpd.DescribePlacementDriverClusterResponseT), nil
}

func (dpd *DescribePDClusterResponse) Marshal(fmt format.Format) ([]byte, error) {
	return marshal(dpd, fmt)
}

func (dpd *DescribePDClusterResponse) Error(status *rpcfb.StatusT) {
	dpd.Status = status
}

func (dpd *DescribePDClusterResponse) OK() {
	dpd.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
}
