package handler

import (
	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster"
	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
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
	logger := h.lg.With(traceutil.TraceLogField(ctx))

	ranges := typeutil.FilterZero[*rpcfb.RangeIdT](req.Ranges)
	sealResponses := make([]*rpcfb.SealRangesResultT, 0, len(ranges))
	ch := make(chan *rpcfb.SealRangesResultT)

	for _, rangeID := range ranges {
		go func(rangeID *rpcfb.RangeIdT) {
			writableRange, err := h.c.SealRange(ctx, rangeID)

			result := &rpcfb.SealRangesResultT{
				Range: writableRange,
			}
			if err != nil {
				logger.Error("failed to seal range", zap.Int64("stream-id", rangeID.StreamId), zap.Int32("range-index", rangeID.RangeIndex), zap.Error(err))
				switch {
				case errors.Is(err, cluster.ErrRangeNotFound):
					result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodePM_SEAL_RANGE_NOT_FOUND, Message: err.Error()}
				case errors.Is(err, cluster.ErrNoDataNodeResponded):
					result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodePM_SEAL_RANGE_NO_DN_RESPONDED, Message: err.Error()}
				case errors.Is(err, cluster.ErrNotEnoughDataNodes):
					result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodePM_NO_AVAILABLE_DN, Message: err.Error()}
				default:
					result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()}
				}
			} else {
				logger.Info("range sealed", zap.Int64("stream-id", rangeID.StreamId), zap.Int32("range-index", rangeID.RangeIndex),
					zap.Int64("range-end-offset", writableRange.StartOffset)) // writableRange.StartOffset == sealedRange.EndOffset
				result.Status = &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK}
			}
			ch <- result
		}(rangeID)
	}

	for range ranges {
		sealResponses = append(sealResponses, <-ch)
	}
	resp.SealResponses = sealResponses
	resp.OK()
}
