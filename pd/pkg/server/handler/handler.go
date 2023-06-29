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
	cluster.RangeServer
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
// If not, it sets "PD_NOT_LEADER" error in the response and returns false
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
	pdCluster := h.pdCluster(ctx)
	return &rpcfb.StatusT{Code: rpcfb.ErrorCodePD_NOT_LEADER, Message: "not leader", Detail: fbutil.Marshal(pdCluster)}
}

func (h *Handler) pdCluster(ctx context.Context) *rpcfb.PlacementDriverClusterT {
	pd := &rpcfb.PlacementDriverClusterT{Nodes: make([]*rpcfb.PlacementDriverNodeT, 0)}
	members, err := h.c.ClusterInfo(ctx)
	if err != nil {
		return &rpcfb.PlacementDriverClusterT{Nodes: []*rpcfb.PlacementDriverNodeT{}}
	}
	for _, member := range members {
		pd.Nodes = append(pd.Nodes, &rpcfb.PlacementDriverNodeT{
			Name:          member.Name,
			AdvertiseAddr: member.AdvertisePDAddr,
			IsLeader:      member.IsLeader,
		})
	}
	return pd
}
