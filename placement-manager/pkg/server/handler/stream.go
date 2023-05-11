package handler

import (
	"github.com/pkg/errors"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster"
)

func (h *Handler) CreateStream(req *protocol.CreateStreamRequest, resp *protocol.CreateStreamResponse) {
	ctx := req.Context()

	if req.Stream == nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: "stream is nil"})
		return
	}

	stream, err := h.c.CreateStream(ctx, req.Stream)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotEnoughDataNodes):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_NO_AVAILABLE_DN, Message: err.Error()})
		case errors.Is(err, cluster.ErrNotLeader):
			resp.Error(h.notLeaderError())
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}

	resp.Stream = stream
	resp.OK()
}

func (h *Handler) DeleteStream(req *protocol.DeleteStreamRequest, resp *protocol.DeleteStreamResponse) {
	ctx := req.Context()

	stream, err := h.c.DeleteStream(ctx, req.StreamId)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			resp.Error(h.notLeaderError())
		case errors.Is(err, cluster.ErrStreamNotFound):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeNOT_FOUND, Message: err.Error()})
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}

	resp.Stream = stream
	resp.OK()
}

func (h *Handler) UpdateStream(req *protocol.UpdateStreamRequest, resp *protocol.UpdateStreamResponse) {
	ctx := req.Context()

	stream, err := h.c.UpdateStream(ctx, req.Stream)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			resp.Error(h.notLeaderError())
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}

	resp.Stream = stream
	resp.OK()
}

func (h *Handler) DescribeStream(req *protocol.DescribeStreamRequest, resp *protocol.DescribeStreamResponse) {
	ctx := req.Context()

	stream, err := h.c.DescribeStream(ctx, req.StreamId)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			resp.Error(h.notLeaderError())
		case errors.Is(err, cluster.ErrStreamNotFound):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeNOT_FOUND, Message: err.Error()})
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}

	resp.Stream = stream
	resp.OK()
}
