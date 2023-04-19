package handler

import (
	"github.com/pkg/errors"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster"
	"github.com/AutoMQ/placement-manager/pkg/util/typeutil"
)

func (h *Handler) CreateStreams(req *protocol.CreateStreamsRequest, resp *protocol.CreateStreamsResponse) {
	ctx := req.Context()

	streams := typeutil.FilterZero[*rpcfb.StreamT](req.Streams)
	results, err := h.c.CreateStreams(ctx, streams)
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

	for _, result := range results {
		result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
	}
	resp.CreateResponses = results
	resp.OK()
}

func (h *Handler) DeleteStreams(req *protocol.DeleteStreamsRequest, resp *protocol.DeleteStreamsResponse) {
	ctx := req.Context()

	streamIDs := make([]int64, 0, len(req.Streams))
	for _, stream := range req.Streams {
		if stream == nil {
			continue
		}
		streamIDs = append(streamIDs, stream.StreamId)
	}
	streams, err := h.c.DeleteStreams(ctx, streamIDs)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			resp.Error(h.notLeaderError())
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}

	resp.DeleteResponses = make([]*rpcfb.DeleteStreamResultT, 0, len(streams))
	for _, stream := range streams {
		resp.DeleteResponses = append(resp.DeleteResponses, &rpcfb.DeleteStreamResultT{
			DeletedStream: stream,
			Status:        &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK},
		})
	}
	resp.OK()
}

func (h *Handler) UpdateStreams(req *protocol.UpdateStreamsRequest, resp *protocol.UpdateStreamsResponse) {
	ctx := req.Context()

	streams := typeutil.FilterZero[*rpcfb.StreamT](req.Streams)
	upStreams, err := h.c.UpdateStreams(ctx, streams)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			resp.Error(h.notLeaderError())
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}

	resp.UpdateResponses = make([]*rpcfb.UpdateStreamResultT, 0, len(upStreams))
	for _, stream := range upStreams {
		resp.UpdateResponses = append(resp.UpdateResponses, &rpcfb.UpdateStreamResultT{
			Stream: stream,
			Status: &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK},
		})
	}
	resp.OK()
}

func (h *Handler) DescribeStreams(req *protocol.DescribeStreamsRequest, resp *protocol.DescribeStreamsResponse) {
	ctx := req.Context()

	result, err := h.c.DescribeStreams(ctx, req.StreamIds)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			resp.Error(h.notLeaderError())
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}

	resp.DescribeResponses = result
	resp.OK()
}
