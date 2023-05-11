package handler

import (
	"fmt"

	"github.com/pkg/errors"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster"
)

func (h *Handler) ListRange(req *protocol.ListRangeRequest, resp *protocol.ListRangeResponse) {
	ctx := req.Context()

	if req.Criteria == nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: "criteria is nil"})
		return
	}

	ranges, err := h.c.ListRange(ctx, req.Criteria)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			resp.Error(h.notLeaderError())
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}

	resp.Ranges = ranges
	resp.OK()
}

func (h *Handler) SealRange(req *protocol.SealRangeRequest, resp *protocol.SealRangeResponse) {
	ctx := req.Context()

	if req.Kind != rpcfb.SealKindPLACEMENT_MANAGER {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: fmt.Sprintf("invalid seal kind: %s", req.Kind)})
		return
	}
	if req.Range == nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: "range is nil"})
		return
	}

	writableRange, err := h.c.SealRange(ctx, req.Range, false)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			resp.Error(h.notLeaderError())
		case errors.Is(err, cluster.ErrRangeNotFound):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeRANGE_NOT_FOUND, Message: err.Error()})
		case errors.Is(err, cluster.ErrRangeAlreadySealed):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeRANGE_ALREADY_SEALED, Message: err.Error()})
		case errors.Is(err, cluster.ErrInvalidEndOffset):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: err.Error()})
		case errors.Is(err, cluster.ErrNotEnoughDataNodes):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_NO_AVAILABLE_DN, Message: err.Error()})
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}

	resp.Range = writableRange
	resp.OK()
}
