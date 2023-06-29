package handler

import (
	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/sbp/protocol"
	"github.com/AutoMQ/pd/pkg/server/cluster"
	"github.com/AutoMQ/pd/pkg/util/traceutil"
)

func (h *Handler) Heartbeat(req *protocol.HeartbeatRequest, resp *protocol.HeartbeatResponse) {
	ctx := req.Context()

	resp.ClientId = req.ClientId
	resp.ClientRole = req.ClientRole
	resp.RangeServer = req.RangeServer

	if req.ClientRole == rpcfb.ClientRoleCLIENT_ROLE_RANGE_SERVER && req.RangeServer != nil {
		err := h.c.Heartbeat(ctx, req.RangeServer)
		if err != nil {
			switch {
			case errors.Is(err, cluster.ErrNotLeader):
				// It's ok to ignore this error, as all pd nodes will receive this request.
				break
			default:
				resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePD_INTERNAL_SERVER_ERROR, Message: err.Error()})
			}
			return
		}
	}
	resp.OK()
}

func (h *Handler) AllocateID(req *protocol.IDAllocationRequest, resp *protocol.IDAllocationResponse) {
	ctx := req.Context()
	logger := h.lg.With(traceutil.TraceLogField(ctx))

	id, err := h.c.AllocateID(ctx)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			resp.Error(h.notLeaderError(ctx))
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePD_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}
	resp.Id = id

	logger.Info("allocate id", zap.String("host", req.Host), zap.Int32("allocated-id", id))
	resp.OK()
}

func (h *Handler) ReportMetrics(req *protocol.ReportMetricsRequest, resp *protocol.ReportMetricsResponse) {
	ctx := req.Context()
	resp.RangeServer = req.RangeServer

	if req.RangeServer == nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: "range server is nil"})
		return
	}
	if req.Metrics == nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: "metrics is nil"})
		return
	}

	err := h.c.Metrics(ctx, req.RangeServer, req.Metrics)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			// It's ok to ignore this error, as all pd nodes will receive this request.
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePD_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}
	resp.OK()
}

func (h *Handler) DescribePDCluster(req *protocol.DescribePDClusterRequest, resp *protocol.DescribePDClusterResponse) {
	ctx := req.Context()
	resp.Cluster = h.pdCluster(ctx)
	resp.OK()
}
