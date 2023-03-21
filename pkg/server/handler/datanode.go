package handler

import (
	"context"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
)

func (s *Sbp) Heartbeat(ctx context.Context, req *protocol.HeartbeatRequest, resp *protocol.HeartbeatResponse) {
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
