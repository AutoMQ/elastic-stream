// Copyright 2016 TiKV Project Authors.
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

package etcdutil

import (
	"context"
	"fmt"
	"time"

	"github.com/pkg/errors"
	"go.etcd.io/etcd/api/v3/mvccpb"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.uber.org/zap"
)

// ModRevision in mvccpb.KeyValue
type ModRevision = int64

// GetOne gets KeyValue with key from etcd.
// GetOne will return nil if the specified key is not found
// GetOne will return an error if etcd returns multiple KeyValue
func GetOne(ctx context.Context, kv clientv3.KV, key []byte, logger *zap.Logger) (*mvccpb.KeyValue, error) {
	resp, err := Get(ctx, kv, key, logger)
	if err != nil {
		return nil, errors.Wrap(err, "get value from etcd")
	}

	if n := len(resp.Kvs); n == 0 {
		return nil, nil
	} else if n > 1 {
		return nil, fmt.Errorf("etcd get multiple values, expected only one. response %v", resp.Kvs)
	}

	return resp.Kvs[0], nil
}

// Get returns the etcd GetResponse by given key and options
func Get(ctx context.Context, kv clientv3.KV, k []byte, logger *zap.Logger, opts ...clientv3.OpOption) (*clientv3.GetResponse, error) {
	ctx, cancel := context.WithTimeout(ctx, DefaultRequestTimeout)
	defer cancel()

	key := string(k)

	start := time.Now()
	resp, err := kv.Get(ctx, key, opts...)
	if cost := time.Since(start); cost > DefaultSlowRequestTime {
		logger.Warn("getting value is too slow", zap.String("key", key), zap.Duration("cost", cost), zap.Error(err))
	}

	if err != nil {
		logger.Error("failed to get value", zap.String("key", key), zap.Error(err))
		return resp, errors.Wrapf(err, "get value by key %s", key)
	}

	return resp, nil
}
