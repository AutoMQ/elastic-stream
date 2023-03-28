package handler

import (
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
)

func (s *Sbp) Heartbeat(req *protocol.HeartbeatRequest, resp *protocol.HeartbeatResponse) {
	ctx := req.Context()

	resp.ClientId = req.ClientId
	resp.ClientRole = req.ClientRole
	resp.DataNode = req.DataNode
	if !s.c.IsLeader() {
		s.notLeaderError(ctx, resp)
		return
	}

	if req.ClientRole == rpcfb.ClientRoleCLIENT_ROLE_DATA_NODE {
		err := s.c.Heartbeat(ctx, req.DataNode)
		if err != nil {
			resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
			return
		}
	}
	resp.OK()
}

func (s *Sbp) AllocateID(req *protocol.IDAllocationRequest, resp *protocol.IDAllocationResponse) {
	ctx := req.Context()
	if !s.c.IsLeader() {
		s.notLeaderError(ctx, resp)
		return
	}
	logger := s.lg.With(traceutil.TraceLogField(ctx))

	id, err := s.c.AllocateID(ctx)
	if err != nil {
		resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR, Message: err.Error()})
		return
	}
	resp.Id = id

	logger.Info("allocate id", zap.String("host", req.Host), zap.Int32("allocated-id", id))
	resp.OK()
}
