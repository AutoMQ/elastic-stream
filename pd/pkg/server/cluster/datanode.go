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
	// ErrNotEnoughDataNodes is returned when there are not enough data nodes to allocate a range.
	ErrNotEnoughDataNodes = errors.New("not enough data nodes")
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

	updated, old := c.cache.SaveDataNode(&cache.RangeServer{
		RangeServerT:   *node,
		LastActiveTime: time.Now(),
	})
	if updated && c.IsLeader() {
		logger.Info("data node updated, start to save it", zap.Any("new", node), zap.Any("old", old))
		_, err := c.storage.SaveDataNode(ctx, node)
		logger.Info("finish saving data node", zap.Int32("node-id", node.ServerId), zap.Error(err))
		if err != nil {
			if errors.Is(err, kv.ErrTxnFailed) {
				return ErrNotLeader
			}
			return err
		}
	}
	return nil
}

// AllocateID allocates a data node id from the id allocator.
// It returns ErrNotLeader if the current PD node is not the leader.
func (c *RaftCluster) AllocateID(ctx context.Context) (int32, error) {
	logger := c.lg.With(traceutil.TraceLogField(ctx))

	id, err := c.dnAlloc.Alloc(ctx)
	if err != nil {
		logger.Error("failed to allocate data node id", zap.Error(err))
		if errors.Is(err, kv.ErrTxnFailed) {
			err = ErrNotLeader
		}
		return -1, err
	}

	return int32(id), nil
}

// Metrics receives metrics from data nodes.
// It returns ErrNotLeader if the current PD node is not the leader.
func (c *RaftCluster) Metrics(ctx context.Context, node *rpcfb.RangeServerT, metrics *rpcfb.RangeServerMetricsT) error {
	logger := c.lg.With(traceutil.TraceLogField(ctx))

	updated, old := c.cache.SaveDataNode(&cache.RangeServer{
		RangeServerT:   *node,
		LastActiveTime: time.Now(),
		Metrics:        metrics,
	})
	if updated && c.IsLeader() {
		logger.Info("data node updated when reporting metrics, start to save it", zap.Any("new", node), zap.Any("old", old))
		_, err := c.storage.SaveDataNode(ctx, node)
		logger.Info("finish saving data node", zap.Int32("node-id", node.ServerId), zap.Error(err))
		if err != nil {
			if errors.Is(err, kv.ErrTxnFailed) {
				return ErrNotLeader
			}
			return err
		}
	}

	return nil
}

// chooseDataNodes selects `cnt` number of data nodes from the available data nodes for a range.
// Only RangeServerT.ServerId is filled in the returned RangeServerT.
// It returns ErrNotEnoughDataNodes if there are not enough data nodes to allocate.
func (c *RaftCluster) chooseDataNodes(cnt int) ([]*rpcfb.RangeServerT, error) {
	if cnt <= 0 {
		return nil, nil
	}

	nodes := c.cache.ActiveDataNodes(c.cfg.DataNodeTimeout)
	if cnt > len(nodes) {
		return nil, errors.Wrapf(ErrNotEnoughDataNodes, "required %d, available %d", cnt, len(nodes))
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

func (c *RaftCluster) fillDataNodesInfo(nodes []*rpcfb.RangeServerT) {
	for _, node := range nodes {
		c.fillDataNodeInfo(node)
	}
}

func (c *RaftCluster) fillDataNodeInfo(node *rpcfb.RangeServerT) {
	if node == nil {
		return
	}
	n := c.cache.RangeServer(node.ServerId)
	if n == nil {
		c.lg.Warn("data node not found", zap.Int32("node-id", node.ServerId))
		return
	}
	node.AdvertiseAddr = n.AdvertiseAddr
}

func eraseDataNodesInfo(o []*rpcfb.RangeServerT) (n []*rpcfb.RangeServerT) {
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
