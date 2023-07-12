package server

import (
	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/sbp/protocol"
)

// Handler responds to a request
type Handler interface {
	// Heartbeat handles heartbeat requests.
	Heartbeat(req *protocol.HeartbeatRequest, resp *protocol.HeartbeatResponse)
	// AllocateID allocates a unique ID for range servers.
	AllocateID(req *protocol.IDAllocationRequest, resp *protocol.IDAllocationResponse)
	// ListRange lists ranges either in a specified steam, a specified range server, or both.
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
	// ReportMetrics reports metrics to pd.
	ReportMetrics(req *protocol.ReportMetricsRequest, resp *protocol.ReportMetricsResponse)
	// DescribePDCluster describes pd cluster membership.
	DescribePDCluster(req *protocol.DescribePDClusterRequest, resp *protocol.DescribePDClusterResponse)
}

var (
	_actionMap = map[rpcfb.OperationCode]Action{
		rpcfb.OperationCodeHEARTBEAT: {
			newReq:  func() protocol.InRequest { return &protocol.HeartbeatRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.HeartbeatResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.Heartbeat(req.(*protocol.HeartbeatRequest), resp.(*protocol.HeartbeatResponse))
			},
		},
		rpcfb.OperationCodeALLOCATE_ID: {
			newReq:  func() protocol.InRequest { return &protocol.IDAllocationRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.IDAllocationResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.AllocateID(req.(*protocol.IDAllocationRequest), resp.(*protocol.IDAllocationResponse))
			},
		},
		rpcfb.OperationCodeLIST_RANGE: {
			newReq:  func() protocol.InRequest { return &protocol.ListRangeRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.ListRangeResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.ListRange(req.(*protocol.ListRangeRequest), resp.(*protocol.ListRangeResponse))
			},
		},
		rpcfb.OperationCodeSEAL_RANGE: {
			newReq:  func() protocol.InRequest { return &protocol.SealRangeRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.SealRangeResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.SealRange(req.(*protocol.SealRangeRequest), resp.(*protocol.SealRangeResponse))
			},
		},
		rpcfb.OperationCodeCREATE_RANGE: {
			newReq:  func() protocol.InRequest { return &protocol.CreateRangeRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.CreateRangeResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.CreateRange(req.(*protocol.CreateRangeRequest), resp.(*protocol.CreateRangeResponse))
			},
		},
		rpcfb.OperationCodeCREATE_STREAM: {
			newReq:  func() protocol.InRequest { return &protocol.CreateStreamRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.CreateStreamResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.CreateStream(req.(*protocol.CreateStreamRequest), resp.(*protocol.CreateStreamResponse))
			},
		},
		rpcfb.OperationCodeDELETE_STREAM: {
			newReq:  func() protocol.InRequest { return &protocol.DeleteStreamRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.DeleteStreamResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.DeleteStream(req.(*protocol.DeleteStreamRequest), resp.(*protocol.DeleteStreamResponse))
			},
		},
		rpcfb.OperationCodeUPDATE_STREAM: {
			newReq:  func() protocol.InRequest { return &protocol.UpdateStreamRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.UpdateStreamResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.UpdateStream(req.(*protocol.UpdateStreamRequest), resp.(*protocol.UpdateStreamResponse))
			},
		},
		rpcfb.OperationCodeDESCRIBE_STREAM: {
			newReq:  func() protocol.InRequest { return &protocol.DescribeStreamRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.DescribeStreamResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.DescribeStream(req.(*protocol.DescribeStreamRequest), resp.(*protocol.DescribeStreamResponse))
			},
		},
		rpcfb.OperationCodeREPORT_METRICS: {
			newReq:  func() protocol.InRequest { return &protocol.ReportMetricsRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.ReportMetricsResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.ReportMetrics(req.(*protocol.ReportMetricsRequest), resp.(*protocol.ReportMetricsResponse))
			},
		},
		rpcfb.OperationCodeDESCRIBE_PLACEMENT_DRIVER: {
			newReq:  func() protocol.InRequest { return &protocol.DescribePDClusterRequest{} },
			newResp: func() protocol.OutResponse { return &protocol.DescribePDClusterResponse{} },
			act: func(handler Handler, req protocol.InRequest, resp protocol.OutResponse) {
				handler.DescribePDCluster(req.(*protocol.DescribePDClusterRequest), resp.(*protocol.DescribePDClusterResponse))
			},
		},
	}
	_unsupportedAction = Action{
		newReq:  func() protocol.InRequest { return &protocol.EmptyRequest{} },
		newResp: func() protocol.OutResponse { return &protocol.SystemErrorResponse{} },
		act: func(_ Handler, _ protocol.InRequest, resp protocol.OutResponse) {
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeUNSUPPORTED_OPERATION, Message: "unsupported operation"})
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
func GetAction(op rpcfb.OperationCode) *Action {
	if action, ok := _actionMap[op]; ok {
		return &action
	}
	if _, ok := rpcfb.EnumNamesOperationCode[op]; !ok || op == rpcfb.OperationCodeUNKNOWN {
		return &_unknownAction
	}
	return &_unsupportedAction
}
