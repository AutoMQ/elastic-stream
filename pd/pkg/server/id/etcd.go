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

	"github.com/AutoMQ/pd/pkg/util/etcdutil"
	"github.com/AutoMQ/pd/pkg/util/traceutil"
	"github.com/AutoMQ/pd/pkg/util/typeutil"
)

const (
	_keySeparator = "/"
	_pathPrefix   = "id-alloc"
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

	kv      clientv3.KV
	cmpFunc func() clientv3.Cmp
	path    string
	start   uint64
	step    uint64

	lg *zap.Logger
}

// EtcdAllocatorParam is the parameter for creating a new etcd allocator.
type EtcdAllocatorParam struct {
	KV       clientv3.KV
	CmpFunc  func() clientv3.Cmp // CmpFunc is used to create a transaction. If CmpFunc is nil, the transaction will not have any additional condition.
	RootPath string              // RootPath is the prefix of all keys in etcd.
	Key      string              // Key is the unique key to identify the allocator.
	Start    uint64              // Start is the start ID of the allocator. If Start is 0, it will be set to _defaultStart.
	Step     uint64              // Step is the step to allocate the next ID. If Step is 0, it will be set to _defaultStep.
}

// NewEtcdAllocator creates a new etcd allocator.
func NewEtcdAllocator(param *EtcdAllocatorParam, lg *zap.Logger) *EtcdAllocator {
	e := &EtcdAllocator{
		kv:      param.KV,
		cmpFunc: param.CmpFunc,
		path:    strings.Join([]string{param.RootPath, _pathPrefix, param.Key}, _keySeparator),
		start:   param.Start,
		step:    param.Step,
	}
	e.lg = lg.With(zap.String("etcd-id-allocator-path", e.path))

	if e.step == 0 {
		e.step = _defaultStep
	}
	if e.start == 0 {
		e.start = _defaultStart
	}
	e.base = e.start
	e.end = e.start
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

	ids := make([]uint64, 0, n)
	if e.end-e.base >= uint64(n) {
		for i := 0; i < n; i++ {
			ids = append(ids, e.base)
			e.base++
		}
		return ids, nil
	}

	for e.end-e.base > 0 {
		ids = append(ids, e.base)
		e.base++
		n--
	}

	growth := e.step * (uint64(n)/e.step + 1)
	if err := e.growLocked(ctx, growth); err != nil {
		return nil, errors.Wrapf(err, "grow %d", growth)
	}

	for i := 0; i < n; i++ {
		ids = append(ids, e.base)
		e.base++
	}
	return ids, nil
}

func (e *EtcdAllocator) growLocked(ctx context.Context, growth uint64) error {
	logger := e.lg.With(traceutil.TraceLogField(ctx))

	kv, err := etcdutil.GetOne(ctx, e.kv, []byte(e.path), logger)
	if err != nil {
		return errors.Wrapf(err, "get key %s", e.path)
	}

	var prevEnd uint64
	var cmpList []clientv3.Cmp
	if e.cmpFunc != nil {
		cmpList = append(cmpList, e.cmpFunc())
	}
	if kv == nil {
		prevEnd = e.base
		cmpList = append(cmpList, clientv3.Compare(clientv3.CreateRevision(e.path), "=", 0))
	} else {
		prevEnd, err = typeutil.BytesToUint64(kv.Value)
		if err != nil {
			return errors.Wrapf(err, "parse value %s", string(kv.Value))
		}
		cmpList = append(cmpList, clientv3.Compare(clientv3.Value(e.path), "=", string(kv.Value)))
	}
	end := prevEnd + growth

	v := typeutil.Uint64ToBytes(end)
	txn := etcdutil.NewTxn(ctx, e.kv, logger).If(cmpList...).Then(clientv3.OpPut(e.path, string(v)))
	resp, err := txn.Commit()
	if err != nil {
		return errors.Wrap(err, "update id")
	}
	if !resp.Succeeded {
		// TODO: add retry mechanism.
		// Currently, there is only one allocator on each key.
		// So if the transaction fails, it means the EtcdAllocatorParam.CmpFunc is not satisfied. And there is no need to retry.
		// If we have multiple allocators on the same key, we need to add a retry mechanism.
		return errors.Wrap(ErrTxnFailed, "update id")
	}

	e.end = end
	e.base = prevEnd
	return nil
}

func (e *EtcdAllocator) Reset(ctx context.Context) error {
	logger := e.lg.With(traceutil.TraceLogField(ctx))

	e.mu.Lock()
	defer e.mu.Unlock()

	txn := etcdutil.NewTxn(ctx, e.kv, logger)
	v := typeutil.Uint64ToBytes(e.start)
	if e.cmpFunc != nil {
		txn = txn.If(e.cmpFunc())
	}
	resp, err := txn.Then(clientv3.OpPut(e.path, string(v))).Commit()
	if err != nil {
		return errors.Wrap(err, "reset etcd id allocator")
	}
	if !resp.Succeeded {
		return errors.Wrap(ErrTxnFailed, "reset etcd id allocator")
	}

	e.base = e.start
	e.end = e.start
	return nil
}

func (e *EtcdAllocator) Logger() *zap.Logger {
	return e.lg
}
