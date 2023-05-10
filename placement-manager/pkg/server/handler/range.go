package handler

import (
	"fmt"

	"github.com/pkg/errors"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster"
	"github.com/AutoMQ/placement-manager/pkg/util/typeutil"
)

func (h *Handler) ListRanges(req *protocol.ListRangesRequest, resp *protocol.ListRangesResponse) {
	ctx := req.Context()

	criteriaList := typeutil.FilterZero[*rpcfb.RangeCriteriaT](req.RangeCriteria)
	listResponses := make([]*rpcfb.ListRangesResultT, 0, len(criteriaList))
	for _, owner := range criteriaList {
		ranges, err := h.c.ListRanges(ctx, owner)

		result := &rpcfb.ListRangesResultT{
			RangeCriteria: owner,
		}
		if err != nil {
			switch {
			case errors.Is(err, cluster.ErrNotLeader):
				resp.Error(h.notLeaderError())
				return
			default:
				result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()}
			}
		} else {
			result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
			result.Ranges = ranges
		}

		listResponses = append(listResponses, result)
	}
	resp.ListResponses = listResponses
	resp.OK()
}

func (h *Handler) SealRanges(req *protocol.SealRangesRequest, resp *protocol.SealRangesResponse) {
	ctx := req.Context()

	entries := typeutil.FilterZero[*rpcfb.SealRangeEntryT](req.Entries)
	sealResults := make([]*rpcfb.SealRangesResultT, 0, len(entries))

	for _, entry := range entries {
		if entry.Type != rpcfb.SealTypePLACEMENT_MANAGER {
			sealResults = append(sealResults, &rpcfb.SealRangesResultT{
				Status: &rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: fmt.Sprintf("invalid seal type: %s", entry.Type)},
			})
			continue
		}
		if entry.Range == nil {
			sealResults = append(sealResults, &rpcfb.SealRangesResultT{
				Status: &rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: "range is nil"},
			})
			continue
		}

		writableRange, err := h.c.SealRange(ctx, entry)

		result := &rpcfb.SealRangesResultT{
			Range: writableRange,
		}
		if err != nil {
			switch {
			case errors.Is(err, cluster.ErrNotLeader):
				resp.Error(h.notLeaderError())
				return
			case errors.Is(err, cluster.ErrRangeNotFound):
				result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeRANGE_NOT_FOUND, Message: err.Error()}
			case errors.Is(err, cluster.ErrRangeAlreadySealed):
				result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeRANGE_ALREADY_SEALED, Message: err.Error()}
			case errors.Is(err, cluster.ErrInvalidEndOffset):
				result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: err.Error()}
			case errors.Is(err, cluster.ErrNotEnoughDataNodes):
				result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodePM_NO_AVAILABLE_DN, Message: err.Error()}
			default:
				result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()}
			}
		} else {
			result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
		}

		sealResults = append(sealResults, result)
	}

	resp.Results = sealResults
	resp.OK()
}
