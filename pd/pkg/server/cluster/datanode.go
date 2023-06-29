package cluster

import (
	"context"
	"math/rand"
	"time"

	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/server/cluster/cache"
	"github.com/AutoMQ/pd/pkg/server/storage/kv"
	"github.com/AutoMQ/pd/pkg/util/traceutil"
)

var (
	// ErrNotEnoughRangeServers is returned when there are not enough range servers to allocate a range.
	ErrNotEnoughRangeServers = errors.New("not enough range servers")
)

type RangeServer interface {
	Heartbeat(ctx context.Context, node *rpcfb.RangeServerT) error
	AllocateID(ctx context.Context) (int32, error)
	Metrics(ctx context.Context, node *rpcfb.RangeServerT, metrics *rpcfb.RangeServerMetricsT) error
}

// Heartbeat updates RangeServer's last active time, and save it to storage if its info changed.
// It returns ErrNotLeader if the current PD node is not the leader.
func (c *RaftCluster) Heartbeat(ctx context.Context, node *rpcfb.RangeServerT) error {
	logger := c.lg.With(traceutil.TraceLogField(ctx))

	updated, old := c.cache.SaveRangeServer(&cache.RangeServer{
		RangeServerT:   *node,
		LastActiveTime: time.Now(),
	})
	if updated && c.IsLeader() {
		logger.Info("range server updated, start to save it", zap.Any("new", node), zap.Any("old", old))
		_, err := c.storage.SaveRangeServer(ctx, node)
		logger.Info("finish saving range server", zap.Int32("node-id", node.ServerId), zap.Error(err))
		if err != nil {
			if errors.Is(err, kv.ErrTxnFailed) {
				return ErrNotLeader
			}
			return err
		}
	}
	return nil
}

// AllocateID allocates a range server id from the id allocator.
// It returns ErrNotLeader if the current PD node is not the leader.
func (c *RaftCluster) AllocateID(ctx context.Context) (int32, error) {
	logger := c.lg.With(traceutil.TraceLogField(ctx))

	id, err := c.dnAlloc.Alloc(ctx)
	if err != nil {
		logger.Error("failed to allocate range server id", zap.Error(err))
		if errors.Is(err, kv.ErrTxnFailed) {
			err = ErrNotLeader
		}
		return -1, err
	}

	return int32(id), nil
}

// Metrics receives metrics from range servers.
// It returns ErrNotLeader if the current PD node is not the leader.
func (c *RaftCluster) Metrics(ctx context.Context, node *rpcfb.RangeServerT, metrics *rpcfb.RangeServerMetricsT) error {
	logger := c.lg.With(traceutil.TraceLogField(ctx))

	updated, old := c.cache.SaveRangeServer(&cache.RangeServer{
		RangeServerT:   *node,
		LastActiveTime: time.Now(),
		Metrics:        metrics,
	})
	if updated && c.IsLeader() {
		logger.Info("range server updated when reporting metrics, start to save it", zap.Any("new", node), zap.Any("old", old))
		_, err := c.storage.SaveRangeServer(ctx, node)
		logger.Info("finish saving range server", zap.Int32("node-id", node.ServerId), zap.Error(err))
		if err != nil {
			if errors.Is(err, kv.ErrTxnFailed) {
				return ErrNotLeader
			}
			return err
		}
	}

	return nil
}

// chooseRangeServers selects `cnt` number of range servers from the available range servers for a range.
// Only RangeServerT.ServerId is filled in the returned RangeServerT.
// It returns ErrNotEnoughRangeServers if there are not enough range servers to allocate.
func (c *RaftCluster) chooseRangeServers(cnt int) ([]*rpcfb.RangeServerT, error) {
	if cnt <= 0 {
		return nil, nil
	}

	nodes := c.cache.ActiveRangeServers(c.cfg.RangeServerTimeout)
	if cnt > len(nodes) {
		return nil, errors.Wrapf(ErrNotEnoughRangeServers, "required %d, available %d", cnt, len(nodes))
	}

	perm := rand.Perm(len(nodes))
	chose := make([]*rpcfb.RangeServerT, cnt)
	for i := 0; i < cnt; i++ {
		// select two random nodes and choose the one with higher score
		node1 := nodes[perm[i]]
		id := node1.ServerId
		if cnt+i < len(perm) {
			node2 := nodes[perm[cnt+i]]
			if node2.Score() > node1.Score() {
				id = node2.ServerId
			}
		}
		chose[i] = &rpcfb.RangeServerT{
			ServerId: id,
		}
	}

	idx := c.nodeIdx.Add(uint64(cnt))
	for i := 0; i < cnt; i++ {
		chose[i] = &rpcfb.RangeServerT{
			ServerId: nodes[(idx-uint64(i))%uint64(len(nodes))].ServerId,
		}
	}

	return chose, nil
}

func (c *RaftCluster) fillRangeServersInfo(nodes []*rpcfb.RangeServerT) {
	for _, node := range nodes {
		c.fillRangeServerInfo(node)
	}
}

func (c *RaftCluster) fillRangeServerInfo(node *rpcfb.RangeServerT) {
	if node == nil {
		return
	}
	n := c.cache.RangeServer(node.ServerId)
	if n == nil {
		c.lg.Warn("range server not found", zap.Int32("node-id", node.ServerId))
		return
	}
	node.AdvertiseAddr = n.AdvertiseAddr
}

func eraseRangeServersInfo(o []*rpcfb.RangeServerT) (n []*rpcfb.RangeServerT) {
	if o == nil {
		return
	}
	n = make([]*rpcfb.RangeServerT, len(o))
	for i, node := range o {
		n[i] = &rpcfb.RangeServerT{
			ServerId: node.ServerId,
		}
	}
	return
}
