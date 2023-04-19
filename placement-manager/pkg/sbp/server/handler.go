package server

import (
	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/codec/operation"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
)

// Handler responds to a request
// TODO support streaming
type Handler interface {
	// Heartbeat handles heartbeat requests.
	Heartbeat(req *protocol.HeartbeatRequest, resp *protocol.HeartbeatResponse)
	// AllocateID allocates a unique ID for data nodes.
	AllocateID(req *protocol.IDAllocationRequest, resp *protocol.IDAllocationResponse)
	// ListRanges lists the ranges of a batch of streams. Or it could list the ranges of all the streams in a specific data node.
	ListRanges(req *protocol.ListRangesRequest, resp *protocol.ListRangesResponse)
	// SealRanges seal ranges of a batch of streams
	SealRanges(req *protocol.SealRangesRequest, resp *protocol.SealRangesResponse)
	// CreateStreams creates a batch of streams.
	CreateStreams(req *protocol.CreateStreamsRequest, resp *protocol.CreateStreamsResponse)
	// DeleteStreams deletes a batch of streams.
	DeleteStreams(req *protocol.DeleteStreamsRequest, resp *protocol.DeleteStreamsResponse)
	// UpdateStreams updates a batch of streams.
	UpdateStreams(req *protocol.UpdateStreamsRequest, resp *protocol.UpdateStreamsResponse)
	// DescribeStreams describes a batch of streams.
	DescribeStreams(req *protocol.DescribeStreamsRequest, resp *protocol.DescribeStreamsResponse)
	// DescribePMCluster describes placement manager cluster membership.
	DescribePMCluster(req *protocol.DescribePMClusterRequest, resp *protocol.DescribePMClusterResponse)
}

var (
	_actionMap = map[operation.Operation]Action{
		{Code: operation.OpHeartbeat}: {
			newReq:  func() protocol.InRequest { return &protocol.HeartbeatRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.HeartbeatResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.Heartbeat(req.(*protocol.HeartbeatRequest), resp.(*protocol.HeartbeatResponse))
			},
		},
		{Code: operation.OpAllocateID}: {
			newReq:  func() protocol.InRequest { return &protocol.IDAllocationRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.IDAllocationResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.AllocateID(req.(*protocol.IDAllocationRequest), resp.(*protocol.IDAllocationResponse))
			},
		},
		{Code: operation.OpListRanges}: {
			newReq:  func() protocol.InRequest { return &protocol.ListRangesRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.ListRangesResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.ListRanges(req.(*protocol.ListRangesRequest), resp.(*protocol.ListRangesResponse))
			},
		},
		{Code: operation.OpSealRanges}: {
			newReq:  func() protocol.InRequest { return &protocol.SealRangesRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.SealRangesResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.SealRanges(req.(*protocol.SealRangesRequest), resp.(*protocol.SealRangesResponse))
			},
		},
		{Code: operation.OpCreateStreams}: {
			newReq:  func() protocol.InRequest { return &protocol.CreateStreamsRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.CreateStreamsResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.CreateStreams(req.(*protocol.CreateStreamsRequest), resp.(*protocol.CreateStreamsResponse))
			},
		},
		{Code: operation.OpDeleteStreams}: {
			newReq:  func() protocol.InRequest { return &protocol.DeleteStreamsRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.DeleteStreamsResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.DeleteStreams(req.(*protocol.DeleteStreamsRequest), resp.(*protocol.DeleteStreamsResponse))
			},
		},
		{Code: operation.OpUpdateStreams}: {
			newReq:  func() protocol.InRequest { return &protocol.UpdateStreamsRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.UpdateStreamsResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.UpdateStreams(req.(*protocol.UpdateStreamsRequest), resp.(*protocol.UpdateStreamsResponse))
			},
		},
		{Code: operation.OpDescribeStreams}: {
			newReq:  func() protocol.InRequest { return &protocol.DescribeStreamsRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.DescribeStreamsResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.DescribeStreams(req.(*protocol.DescribeStreamsRequest), resp.(*protocol.DescribeStreamsResponse))
			},
		},
		{Code: operation.OpDescribePMCluster}: {
			newReq:  func() protocol.InRequest { return &protocol.DescribePMClusterRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.DescribePMClusterResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.DescribePMCluster(req.(*protocol.DescribePMClusterRequest), resp.(*protocol.DescribePMClusterResponse))
			},
		},
	}
	_unknownAction = Action{
		newReq:  func() protocol.InRequest { return &protocol.EmptyRequest{} },
		newResp: func() protocol.OutResponse { return &protocol.SystemErrorResponse{} },
		act: func(_ Handler, _ protocol.InRequest, resp protocol.OutResponse) {
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeNOT_FOUND, Message: "unknown operation"})
		},
	}
)

// Action is an action used to handle a request
type Action struct {
	newReq  func() protocol.InRequest
	newResp func() protocol.OutResponse
	act     func(handler Handler, req protocol.InRequest, resp protocol.OutResponse)
}

// GetAction returns the action for the specified operation
func GetAction(op operation.Operation) *Action {
	if action, ok := _actionMap[op]; ok {
		return &action
	}
	return &_unknownAction
}
