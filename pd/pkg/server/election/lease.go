// Copyright 2019 TiKV Project Authors.
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
	"time"

	"github.com/pkg/errors"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.uber.org/zap"

	"github.com/AutoMQ/pd/pkg/util/etcdutil"
)

const (
	_revokeLeaseTimeout = time.Second
)

// lease is used as the low-level mechanism for campaigning and renewing elected leadership.
// The way to gain and maintain leadership is to update and keep the lease alive continuously.
type lease struct {
	purpose string // purpose is used to show what this election for

	// etcd client and lease
	client *clientv3.Client
	lease  clientv3.Lease
	ID     clientv3.LeaseID

	// leaseTimeout and expireTime are used to control the lease's lifetime
	leaseTimeout time.Duration
	expireTime   atomic.Pointer[time.Time]
}

// Grant uses `lease.Grant` to initialize the lease and expireTime.
func (l *lease) Grant(leaseTimeout int64, logger *zap.Logger) error {
	start := time.Now()
	ctx, cancel := context.WithTimeout(l.client.Ctx(), etcdutil.DefaultRequestTimeout)
	leaseResp, err := l.lease.Grant(ctx, leaseTimeout)
	cancel()

	if err != nil {
		return errors.Wrap(err, "etcd grant lease")
	}
	if cost := time.Since(start); cost > etcdutil.DefaultSlowRequestTime {
		logger.Warn("lease grants too slow", zap.Duration("cost", cost), zap.String("purpose", l.purpose))
	}
	logger.Info("lease granted", zap.Int64("lease-id", int64(leaseResp.ID)), zap.Int64("lease-timeout", leaseTimeout), zap.String("purpose", l.purpose))

	l.ID = leaseResp.ID
	l.leaseTimeout = time.Duration(leaseTimeout) * time.Second
	eTime := start.Add(time.Duration(leaseResp.TTL) * time.Second)
	l.expireTime.Store(&eTime)
	return nil
}

// KeepAlive auto-renews the lease and update expireTime.
func (l *lease) KeepAlive(ctx context.Context, logger *zap.Logger) {
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	timeCh := l.keepAliveWorker(ctx, l.leaseTimeout/3, logger)

	var maxExpire time.Time
	for {
		select {
		case t := <-timeCh:
			if t.After(maxExpire) {
				maxExpire = t
				// Check again to make sure the `expireTime` still needs to be updated.
				select {
				case <-ctx.Done():
					return
				default:
					l.expireTime.Store(&t)
				}
			}
		case <-time.After(l.leaseTimeout):
			logger.Info("lease timeout", zap.Time("expire", *l.expireTime.Load()), zap.String("purpose", l.purpose))
			return
		case <-ctx.Done():
			return
		}
	}
}

// Close releases the lease.
func (l *lease) Close() {
	// Reset expire time.
	l.expireTime.Store(&time.Time{})

	// Try to revoke lease to make subsequent elections faster.
	ctx, cancel := context.WithTimeout(l.client.Ctx(), _revokeLeaseTimeout)
	defer cancel()
	_, _ = l.lease.Revoke(ctx, l.ID)

	_ = l.lease.Close()
}

// IsExpired checks if the lease is expired. If it returns true,
// current leader should step down and try to re-elect again.
func (l *lease) IsExpired() bool {
	if l.expireTime.Load() == nil {
		return true
	}
	return time.Now().After(*l.expireTime.Load())
}

// Periodically call `lease.KeepAliveOnce` and post back latest received expire time into the channel.
func (l *lease) keepAliveWorker(ctx context.Context, interval time.Duration, logger *zap.Logger) <-chan time.Time {
	ch := make(chan time.Time)

	go func() {
		ticker := time.NewTicker(interval)
		defer ticker.Stop()

		logger.Info("start lease keep alive worker", zap.Duration("interval", interval), zap.String("purpose", l.purpose))
		defer logger.Info("stop lease keep alive worker", zap.String("purpose", l.purpose))

		for {
			go func() {
				start := time.Now()
				ctx1, cancel := context.WithTimeout(ctx, l.leaseTimeout)
				defer cancel()
				res, err := l.lease.KeepAliveOnce(ctx1, l.ID)
				if err != nil {
					logger.Warn("lease keep alive failed", zap.String("purpose", l.purpose), zap.Error(err))
					return
				}
				if res.TTL > 0 {
					expire := start.Add(time.Duration(res.TTL) * time.Second)
					select {
					case ch <- expire:
					case <-ctx1.Done():
					}
				}
			}()

			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
			}
		}
	}()

	return ch
}
