package handler

import (
	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster"
	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
)

func (h *Handler) Heartbeat(req *protocol.HeartbeatRequest, resp *protocol.HeartbeatResponse) {
	ctx := req.Context()

	resp.ClientId = req.ClientId
	resp.ClientRole = req.ClientRole
	resp.DataNode = req.DataNode

	if req.ClientRole == rpcfb.ClientRoleCLIENT_ROLE_DATA_NODE && req.DataNode != nil {
		err := h.c.Heartbeat(ctx, req.DataNode)
		if err != nil {
			switch {
			case errors.Is(err, cluster.ErrNotLeader):
				// It's ok to ignore this error, as all pm nodes will receive this request.
				break
			default:
				resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
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
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}
	resp.Id = id

	logger.Info("allocate id", zap.String("host", req.Host), zap.Int32("allocated-id", id))
	resp.OK()
}

func (h *Handler) ReportMetrics(req *protocol.ReportMetricsRequest, resp *protocol.ReportMetricsResponse) {
	ctx := req.Context()
	resp.DataNode = req.DataNode

	if req.DataNode == nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: "data node is nil"})
		return
	}
	if req.Metrics == nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodeBAD_REQUEST, Message: "metrics is nil"})
		return
	}

	err := h.c.Metrics(ctx, req.DataNode, req.Metrics)
	if err != nil {
		switch {
		case errors.Is(err, cluster.ErrNotLeader):
			// It's ok to ignore this error, as all pm nodes will receive this request.
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}
	resp.OK()
}

func (h *Handler) DescribePMCluster(req *protocol.DescribePMClusterRequest, resp *protocol.DescribePMClusterResponse) {
	ctx := req.Context()
	resp.Cluster = h.pmCluster(ctx)
	resp.OK()
}
