// Copyright 2020 TiKV Project Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

package election

import (
	"context"
	"sync/atomic"

	"github.com/pkg/errors"
	"go.etcd.io/etcd/api/v3/mvccpb"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.uber.org/zap"

	"github.com/AutoMQ/pd/pkg/util/etcdutil"
)

// Leadership is used to manage the leadership campaigning.
type Leadership struct {
	purpose string                // purpose is used to show what this election for
	lease   atomic.Pointer[lease] // The lease which is used to get this leadership
	client  *clientv3.Client

	// leaderKey and leaderValue are key-value pair in etcd
	leaderKey   string
	leaderValue string

	keepAliveCtx        context.Context
	keepAliveCancelFunc context.CancelFunc

	lg *zap.Logger
}

// NewLeadership creates a new Leadership.
func NewLeadership(client *clientv3.Client, leaderKey, purpose string, logger *zap.Logger) *Leadership {
	logger = logger.With(zap.String("leader-key", leaderKey))
	leadership := &Leadership{
		purpose:   purpose,
		client:    client,
		leaderKey: leaderKey,
		lg:        logger,
	}
	return leadership
}

// Watch is used to watch the changes of the leadership, usually is used to
// detect the leadership stepping down and restart an election as soon as possible.
func (ls *Leadership) Watch(serverCtx context.Context, revision etcdutil.ModRevision) {
	logger := ls.lg

	watcher := clientv3.NewWatcher(ls.client)
	defer func(watcher clientv3.Watcher) {
		err := watcher.Close()
		if err != nil {
			logger.Error("failed to close watcher", zap.Error(err))
		}
	}(watcher)

	ctx, cancel := context.WithCancel(serverCtx)
	defer cancel()

	// The revision is the revision of last modification on this key.
	// If the revision is compacted, will meet required revision has been compacted error.
	// In this case, use the compact revision to re-watch the key.
	for {
		rch := watcher.Watch(ctx, ls.leaderKey, clientv3.WithRev(revision))
		for watchResp := range rch {
			// meet compacted error, use the compact revision.
			if watchResp.CompactRevision != 0 {
				logger.Warn("required revision has been compacted, use the compact revision",
					zap.Int64("required-revision", revision), zap.Int64("compact-revision", watchResp.CompactRevision))
				revision = watchResp.CompactRevision
				break
			}
			if watchResp.Canceled {
				logger.Error("leadership watcher is canceled with", zap.Int64("revision", revision),
					zap.String("leader-key", ls.leaderKey), zap.String("purpose", ls.purpose), zap.Error(watchResp.Err()))
				return
			}

			for _, ev := range watchResp.Events {
				if ev.Type == mvccpb.DELETE {
					logger.Info("current leadership is deleted",
						zap.String("leader-key", ls.leaderKey), zap.String("purpose", ls.purpose))
					return
				}
			}
		}

		select {
		case <-ctx.Done():
			// server closed, return
			return
		default:
		}
	}
}

// Campaign is used to campaign the leader with given lease and returns a leadership
// returns true if successfully campaign leader
func (ls *Leadership) Campaign(ctx context.Context, leaseTimeout int64, leaderData string) (bool, error) {
	logger := ls.lg

	ls.leaderValue = leaderData
	// Create a new lease to campaign
	newLease := &lease{
		purpose: ls.purpose,
		client:  ls.client,
		lease:   clientv3.NewLease(ls.client),
	}
	ls.lease.Store(newLease)

	if err := newLease.Grant(leaseTimeout, ls.lg); err != nil {
		return false, errors.Wrap(err, "grant lease")
	}

	resp, err := etcdutil.NewTxn(ctx, ls.client, logger).
		// The leader key must not exist, so the CreateRevision is 0.
		If(clientv3.Compare(clientv3.CreateRevision(ls.leaderKey), "=", 0)).
		Then(clientv3.OpPut(ls.leaderKey, leaderData, clientv3.WithLease(newLease.ID))).
		Commit()

	logger.Info("check campaign resp", zap.Any("response", resp))
	if err != nil {
		newLease.Close()
		logger.Error("failed to set leader info", zap.String("leader-key", ls.leaderKey),
			zap.String("leader-info", leaderData), zap.Int64("lease-id", int64(newLease.ID)), zap.Error(err))
		return false, errors.Wrap(err, "etcd transaction: compare and put leader info")
	}
	if !resp.Succeeded {
		newLease.Close()
		logger.Info("etcd transaction failed: leader key already exists")
		return false, nil
	}
	logger.Info("campaign leader success", zap.String("leader-path", ls.leaderKey), zap.String("purpose", ls.purpose))
	return true, nil
}

// Keep will keep the leadership available by update the lease's expired time continuously
func (ls *Leadership) Keep(ctx context.Context) {
	if ls == nil {
		return
	}
	ls.keepAliveCtx, ls.keepAliveCancelFunc = context.WithCancel(ctx)

	l := ls.getLease()
	if l != nil {
		go l.KeepAlive(ls.keepAliveCtx, ls.lg)
	}
}

// DeleteLeaderKey deletes the corresponding leader from etcd by the leaderPath as the key.
func (ls *Leadership) DeleteLeaderKey(ctx context.Context) error {
	logger := ls.lg

	resp, err := etcdutil.NewTxn(ctx, ls.client, logger).Then(clientv3.OpDelete(ls.leaderKey)).Commit()
	if err != nil {
		logger.Error("failed to delete leader key", zap.String("leader-key", ls.leaderKey))
		return errors.Wrap(err, "delete etcd key")
	}
	if !resp.Succeeded {
		logger.Error("failed to delete etcd key, transaction failed", zap.String("leader-key", ls.leaderKey))
		return errors.Errorf("failed to delete etcd key %s: transaction failed", ls.leaderKey)
	}

	// Reset the lease as soon as possible.
	ls.Reset()
	logger.Info("delete the leader key ok", zap.String("leader-path", ls.leaderKey), zap.String("purpose", ls.purpose))
	return nil
}

// Reset does some defer jobs such as closing lease, resetting lease etc.
func (ls *Leadership) Reset() {
	l := ls.getLease()
	if l == nil {
		return
	}

	if ls.keepAliveCancelFunc != nil {
		ls.keepAliveCancelFunc()
	}
	l.Close()
}

// Check returns whether the leadership is still available.
func (ls *Leadership) Check() bool {
	l := ls.getLease()
	return l != nil && !l.IsExpired()
}

// getLease gets the lease of leadership, only if leadership is valid,
// i.e. the owner is a true leader, the lease is not nil.
// ATTENTION: getLease may return nil
func (ls *Leadership) getLease() *lease {
	l := ls.lease.Load()
	if l == nil {
		return nil
	}
	return l
}
