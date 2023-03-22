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

package id

import (
	"context"
	"strings"
	"sync"

	"github.com/pkg/errors"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/pkg/util/etcdutil"
	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
	"github.com/AutoMQ/placement-manager/pkg/util/typeutil"
)

const (
	_keySeparator = "/"
)

var (
	// ErrTxnFailed is the error when etcd transaction failed.
	ErrTxnFailed = errors.New("etcd transaction failed")
)

// EtcdAllocator is an allocator based on etcd.
type EtcdAllocator struct {
	mu   sync.Mutex
	base uint64
	end  uint64

	client     *clientv3.Client
	path       string
	newTxnFunc func(ctx context.Context) clientv3.Txn
	start      uint64
	step       uint64

	lg *zap.Logger
}

// EtcdAllocatorParam is the parameter for creating a new etcd allocator.
type EtcdAllocatorParam struct {
	Client   *clientv3.Client
	CmpFunc  func() clientv3.Cmp // CmpFunc is used to create a transaction. If CmpFunc is nil, the transaction will not have any additional condition.
	RootPath string              // RootPath is the prefix of all keys in etcd.
	Key      string              // Key is the unique key to identify the allocator.
	Start    uint64              // Start is the start ID of the allocator. If Start is 0, it will be set to _defaultStart.
	Step     uint64              // Step is the step to allocate the next ID. If Step is 0, it will be set to _defaultStep.
}

// NewEtcdAllocator creates a new etcd allocator.
func NewEtcdAllocator(param *EtcdAllocatorParam, lg *zap.Logger) *EtcdAllocator {
	e := &EtcdAllocator{
		client: param.Client,
		path:   strings.Join([]string{param.RootPath, param.Key}, _keySeparator),
		lg:     lg,
		start:  param.Start,
		step:   param.Step,
	}

	if e.step == 0 {
		e.step = _defaultStep
	}
	if e.start == 0 {
		e.start = _defaultStart
	}
	e.base = e.start
	e.end = e.start

	if param.CmpFunc != nil {
		e.newTxnFunc = func(ctx context.Context) clientv3.Txn {
			// cmpFunc should be evaluated lazily.
			return etcdutil.NewTxn(ctx, param.Client, lg.With(traceutil.TraceLogField(ctx))).If(param.CmpFunc())
		}
	} else {
		e.newTxnFunc = func(ctx context.Context) clientv3.Txn {
			return etcdutil.NewTxn(ctx, param.Client, lg.With(traceutil.TraceLogField(ctx)))
		}
	}
	return e
}

func (e *EtcdAllocator) Alloc(ctx context.Context) (uint64, error) {
	ids, err := e.AllocN(ctx, 1)
	if err != nil {
		return 0, err
	}
	return ids[0], nil
}

func (e *EtcdAllocator) AllocN(ctx context.Context, n int) ([]uint64, error) {
	if n <= 0 {
		return nil, nil
	}

	e.mu.Lock()
	defer e.mu.Unlock()

	if e.end-e.base < uint64(n) {
		needs := uint64(n) - (e.end - e.base)
		growth := e.step * (needs/e.step + 1)
		if err := e.growLocked(ctx, growth); err != nil {
			return nil, errors.WithMessagef(err, "grow %d", growth)
		}
	}

	ids := make([]uint64, 0, n)
	for i := 0; i < n; i++ {
		ids = append(ids, e.base)
		e.base++
	}
	return ids, nil
}

func (e *EtcdAllocator) growLocked(ctx context.Context, growth uint64) error {
	logger := e.lg.With(traceutil.TraceLogField(ctx))

	kv, err := etcdutil.GetOne(ctx, e.client, []byte(e.path), logger)
	if err != nil {
		return errors.WithMessagef(err, "get key %s", e.path)
	}

	var base uint64
	var cmp clientv3.Cmp
	if kv == nil {
		base = e.start
		cmp = clientv3.Compare(clientv3.CreateRevision(e.path), "=", 0)
	} else {
		base, err = typeutil.BytesToUint64(kv.Value)
		if err != nil {
			return errors.WithMessagef(err, "parse value %s", string(kv.Value))
		}
		cmp = clientv3.Compare(clientv3.Value(e.path), "=", string(kv.Value))
	}
	end := base + growth

	v := typeutil.Uint64ToBytes(end)
	txn := e.newTxnFunc(ctx).If(cmp).Then(clientv3.OpPut(e.path, string(v)))
	resp, err := txn.Commit()
	if err != nil {
		return errors.WithMessage(err, "update id")
	}
	if !resp.Succeeded {
		return ErrTxnFailed
	}

	e.base = base
	e.end = base + growth
	return nil
}

func (e *EtcdAllocator) Reset(ctx context.Context) error {
	e.mu.Lock()
	defer e.mu.Unlock()

	v := typeutil.Uint64ToBytes(e.start)
	txn := e.newTxnFunc(ctx).Then(clientv3.OpPut(e.path, string(v)))
	resp, err := txn.Commit()
	if err != nil {
		return errors.WithMessage(err, "reset etcd id allocator")
	}
	if !resp.Succeeded {
		return ErrTxnFailed
	}

	e.base = e.start
	e.end = e.start
	return nil
}

func (e *EtcdAllocator) Logger() *zap.Logger {
	return e.lg
}
