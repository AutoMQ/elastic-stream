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

	resp.Error(&rpcfb.StatusT{Code: rpcfb.ErrorCodePM_NOT_LEADER, Message: "not leader", Detail: h.pmInfo()})
	return false
}

func (h *Handler) pmInfo() []byte {
	pm := &rpcfb.PlacementManagerClusterT{Nodes: make([]*rpcfb.PlacementManagerNodeT, 0, 1)}
	leader := h.c.Leader()
	if leader != nil {
		pm.Nodes = append(pm.Nodes, &rpcfb.PlacementManagerNodeT{
			Name:          leader.Name,
			AdvertiseAddr: leader.SbpAddr,
			IsLeader:      true,
		})
	}
	return fbutil.Marshal(pm)
}
