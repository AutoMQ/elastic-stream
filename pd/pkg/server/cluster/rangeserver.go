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
	Heartbeat(ctx context.Context, rangeServer *rpcfb.RangeServerT) error
	AllocateID(ctx context.Context) (int32, error)
	Metrics(ctx context.Context, rangeServer *rpcfb.RangeServerT, metrics *rpcfb.RangeServerMetricsT) error
}

// Heartbeat updates RangeServer's last active time, and save it to storage if its info changed.
// It returns ErrNotLeader if the current PD node is not the leader.
func (c *RaftCluster) Heartbeat(ctx context.Context, rangeServer *rpcfb.RangeServerT) error {
	logger := c.lg.With(traceutil.TraceLogField(ctx))

	updated, old := c.cache.SaveRangeServer(&cache.RangeServer{
		RangeServerT:   *rangeServer,
		LastActiveTime: time.Now(),
	})
	if updated && c.IsLeader() {
		logger.Info("range server updated, start to save it", zap.Any("new", rangeServer), zap.Any("old", old))
		_, err := c.storage.SaveRangeServer(ctx, rangeServer)
		logger.Info("finish saving range server", zap.Int32("server-id", rangeServer.ServerId), zap.Error(err))
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
func (c *RaftCluster) Metrics(ctx context.Context, rangeServer *rpcfb.RangeServerT, metrics *rpcfb.RangeServerMetricsT) error {
	logger := c.lg.With(traceutil.TraceLogField(ctx))

	updated, old := c.cache.SaveRangeServer(&cache.RangeServer{
		RangeServerT:   *rangeServer,
		LastActiveTime: time.Now(),
		Metrics:        metrics,
	})
	if updated && c.IsLeader() {
		logger.Info("range server updated when reporting metrics, start to save it", zap.Any("new", rangeServer), zap.Any("old", old))
		_, err := c.storage.SaveRangeServer(ctx, rangeServer)
		logger.Info("finish saving range server", zap.Int32("server-id", rangeServer.ServerId), zap.Error(err))
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

	rangeServers := c.cache.ActiveRangeServers(c.cfg.RangeServerTimeout)
	if cnt > len(rangeServers) {
		return nil, errors.Wrapf(ErrNotEnoughRangeServers, "required %d, available %d", cnt, len(rangeServers))
	}

	perm := rand.Perm(len(rangeServers))
	chose := make([]*rpcfb.RangeServerT, cnt)
	for i := 0; i < cnt; i++ {
		// select two random range servers and choose the one with higher score
		rangeServer1 := rangeServers[perm[i]]
		id := rangeServer1.ServerId
		if cnt+i < len(perm) {
			rangeServer2 := rangeServers[perm[cnt+i]]
			if rangeServer2.Score() > rangeServer1.Score() {
				id = rangeServer2.ServerId
			}
		}
		chose[i] = &rpcfb.RangeServerT{
			ServerId: id,
		}
	}

	idx := c.rangeServerIdx.Add(uint64(cnt))
	for i := 0; i < cnt; i++ {
		chose[i] = &rpcfb.RangeServerT{
			ServerId: rangeServers[(idx-uint64(i))%uint64(len(rangeServers))].ServerId,
		}
	}

	return chose, nil
}

func (c *RaftCluster) fillRangeServersInfo(rangeServers []*rpcfb.RangeServerT) {
	for _, rangeServer := range rangeServers {
		c.fillRangeServerInfo(rangeServer)
	}
}

func (c *RaftCluster) fillRangeServerInfo(rangeServer *rpcfb.RangeServerT) {
	if rangeServer == nil {
		return
	}
	rs := c.cache.RangeServer(rangeServer.ServerId)
	if rs == nil {
		c.lg.Warn("range server not found", zap.Int32("server-id", rangeServer.ServerId))
		return
	}
	rangeServer.AdvertiseAddr = rs.AdvertiseAddr
}

func eraseRangeServersInfo(o []*rpcfb.RangeServerT) (rs []*rpcfb.RangeServerT) {
	if o == nil {
		return
	}
	rs = make([]*rpcfb.RangeServerT, len(o))
	for i, rangeServer := range o {
		rs[i] = &rpcfb.RangeServerT{
			ServerId: rangeServer.ServerId,
		}
	}
	return
}
