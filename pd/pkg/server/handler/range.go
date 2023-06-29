package handler

import (
	"fmt"

	"github.com/pkg/errors"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/sbp/protocol"
	"github.com/AutoMQ/pd/pkg/server/cluster"
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
			resp.Error(h.notLeaderError(ctx))
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePD_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}

	resp.Ranges = ranges
	resp.OK()
}

func (h *Handler) SealRange(req *protocol.SealRangeRequest, resp *protocol.SealRangeResponse) {
	ctx := req.Context()

	if req.Kind != rpcfb.SealKindPLACEMENT_DRIVER {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: fmt.Sprintf("invalid seal kind: %s", req.Kind)})
		return
	}
	if req.Range == nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: "range is nil"})
		return
	}

	r, err := h.c.SealRange(ctx, req.Range)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			resp.Error(h.notLeaderError(ctx))
		case errors.Is(err, cluster.ErrStreamNotFound):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeNOT_FOUND, Message: err.Error()})
		case errors.Is(err, cluster.ErrRangeNotFound):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeRANGE_NOT_FOUND, Message: err.Error()})
		case errors.Is(err, cluster.ErrRangeAlreadySealed):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeRANGE_ALREADY_SEALED, Message: err.Error()})
		case errors.Is(err, cluster.ErrInvalidEndOffset):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: err.Error()})
		case errors.Is(err, cluster.ErrExpiredRangeEpoch):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeEXPIRED_RANGE_EPOCH, Message: err.Error()})
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePD_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}

	resp.Range = r
	resp.OK()
}

func (h *Handler) CreateRange(req *protocol.CreateRangeRequest, resp *protocol.CreateRangeResponse) {
	ctx := req.Context()

	if req.Range == nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: "range is nil"})
		return
	}

	r, err := h.c.CreateRange(ctx, req.Range)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			resp.Error(h.notLeaderError(ctx))
		case errors.Is(err, cluster.ErrStreamNotFound):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeNOT_FOUND, Message: err.Error()})
		case errors.Is(err, cluster.ErrInvalidRangeIndex):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: err.Error()})
		case errors.Is(err, cluster.ErrCreateBeforeSeal):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeCREATE_RANGE_BEFORE_SEAL, Message: err.Error()})
		case errors.Is(err, cluster.ErrInvalidStartOffset):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: err.Error()})
		case errors.Is(err, cluster.ErrNotEnoughRangeServers):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePD_NO_AVAILABLE_RS, Message: err.Error()})
		case errors.Is(err, cluster.ErrExpiredRangeEpoch):
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeEXPIRED_RANGE_EPOCH, Message: err.Error()})
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePD_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}

	resp.Range = r
	resp.OK()
}
