package handler

import (
	"context"

	"go.uber.org/zap"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/sbp/protocol"
	"github.com/AutoMQ/pd/pkg/server/cluster"
	"github.com/AutoMQ/pd/pkg/util/fbutil"
	"github.com/AutoMQ/pd/pkg/util/traceutil"
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

	ctx := req.Context()
	h.lg.Warn("not leader", traceutil.TraceLogField(ctx))

	resp.Error(h.notLeaderError(ctx))
	return false
}

func (h *Handler) notLeaderError(ctx context.Context) *rpcfb.StatusT {
	pmCluster := h.pdCluster(ctx)
	return &rpcfb.StatusT{Code: rpcfb.ErrorCodePM_NOT_LEADER, Message: "not leader", Detail: fbutil.Marshal(pmCluster)}
}

func (h *Handler) pdCluster(ctx context.Context) *rpcfb.PlacementManagerClusterT {
	pd := &rpcfb.PlacementManagerClusterT{Nodes: make([]*rpcfb.PlacementManagerNodeT, 0)}
	members, err := h.c.ClusterInfo(ctx)
	if err != nil {
		return &rpcfb.PlacementManagerClusterT{Nodes: []*rpcfb.PlacementManagerNodeT{}}
	}
	for _, member := range members {
		pd.Nodes = append(pd.Nodes, &rpcfb.PlacementManagerNodeT{
			Name:          member.Name,
			AdvertiseAddr: member.AdvertisePMAddr,
			IsLeader:      member.IsLeader,
		})
	}
	return pd
}
