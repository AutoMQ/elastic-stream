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
			resp.Error(h.notLeaderError())
		default:
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		}
		return
	}
	resp.Id = id

	logger.Info("allocate id", zap.String("host", req.Host), zap.Int32("allocated-id", id))
	resp.OK()
}
