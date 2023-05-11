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
	// ListRange lists ranges either in a specified steam, a specified data node, or both.
	ListRange(req *protocol.ListRangeRequest, resp *protocol.ListRangeResponse)
	// SealRange seals the specified range.
	SealRange(req *protocol.SealRangeRequest, resp *protocol.SealRangeResponse)
	// CreateRange creates the specified range.
	CreateRange(req *protocol.CreateRangeRequest, resp *protocol.CreateRangeResponse)
	// CreateStream creates a stream.
	CreateStream(req *protocol.CreateStreamRequest, resp *protocol.CreateStreamResponse)
	// DeleteStream deletes the specified stream.
	DeleteStream(req *protocol.DeleteStreamRequest, resp *protocol.DeleteStreamResponse)
	// UpdateStream updates the specified stream.
	UpdateStream(req *protocol.UpdateStreamRequest, resp *protocol.UpdateStreamResponse)
	// DescribeStream describes the specified stream.
	DescribeStream(req *protocol.DescribeStreamRequest, resp *protocol.DescribeStreamResponse)
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
		{Code: operation.OpListRange}: {
			newReq:  func() protocol.InRequest { return &protocol.ListRangeRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.ListRangeResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.ListRange(req.(*protocol.ListRangeRequest), resp.(*protocol.ListRangeResponse))
			},
		},
		{Code: operation.OpSealRange}: {
			newReq:  func() protocol.InRequest { return &protocol.SealRangeRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.SealRangeResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.SealRange(req.(*protocol.SealRangeRequest), resp.(*protocol.SealRangeResponse))
			},
		},
		{Code: operation.OpCreateRange}: {
			newReq:  func() protocol.InRequest { return &protocol.CreateRangeRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.CreateRangeResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.CreateRange(req.(*protocol.CreateRangeRequest), resp.(*protocol.CreateRangeResponse))
			},
		},
		{Code: operation.OpCreateStream}: {
			newReq:  func() protocol.InRequest { return &protocol.CreateStreamRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.CreateStreamResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.CreateStream(req.(*protocol.CreateStreamRequest), resp.(*protocol.CreateStreamResponse))
			},
		},
		{Code: operation.OpDeleteStream}: {
			newReq:  func() protocol.InRequest { return &protocol.DeleteStreamRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.DeleteStreamResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.DeleteStream(req.(*protocol.DeleteStreamRequest), resp.(*protocol.DeleteStreamResponse))
			},
		},
		{Code: operation.OpUpdateStream}: {
			newReq:  func() protocol.InRequest { return &protocol.UpdateStreamRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.UpdateStreamResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.UpdateStream(req.(*protocol.UpdateStreamRequest), resp.(*protocol.UpdateStreamResponse))
			},
		},
		{Code: operation.OpDescribeStream}: {
			newReq:  func() protocol.InRequest { return &protocol.DescribeStreamRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.DescribeStreamResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.DescribeStream(req.(*protocol.DescribeStreamRequest), resp.(*protocol.DescribeStreamResponse))
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
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeUNKNOWN_OPERATION, Message: "unknown operation"})
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
