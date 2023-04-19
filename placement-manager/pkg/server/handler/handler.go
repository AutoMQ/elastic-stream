package handler

import (
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster"
	"github.com/AutoMQ/placement-manager/pkg/util/fbutil"
	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
)

type Cluster interface {
	cluster.DataNode
	cluster.Range
	cluster.Stream
	cluster.Member
}

// Handler is an sbp handler, implements server.Handler
type Handler struct {
	c  Cluster
	lg *zap.Logger
}

// NewHandler creates an sbp handler
func NewHandler(c Cluster, lg *zap.Logger) *Handler {
	return &Handler{
		c:  c,
		lg: lg,
	}
}

// Check checks if the current node is the leader
// If not, it sets "PM_NOT_LEADER" error in the response and returns false
func (h *Handler) Check(req protocol.InRequest, resp protocol.OutResponse) (pass bool) {
	if h.c.IsLeader() {
		return true
	}
	h.lg.Warn("not leader", traceutil.TraceLogField(req.Context()))

	resp.Error(h.notLeaderError())
	return false
}

func (h *Handler) notLeaderError() *rpcfb.StatusT {
	pmCluster := h.pmCluster()
	return &rpcfb.StatusT{Code: rpcfb.ErrorCodePM_NOT_LEADER, Message: "not leader", Detail: fbutil.Marshal(pmCluster)}
}

func (h *Handler) pmCluster() *rpcfb.PlacementManagerClusterT {
	pm := &rpcfb.PlacementManagerClusterT{Nodes: make([]*rpcfb.PlacementManagerNodeT, 0)}
	for _, member := range h.c.ClusterInfo() {
		pm.Nodes = append(pm.Nodes, &rpcfb.PlacementManagerNodeT{
			Name:          member.Name,
			AdvertiseAddr: member.SbpAddr,
		})
	}
	if len(pm.Nodes) > 0 {
		pm.Nodes[0].IsLeader = true
	}
	return pm
}
