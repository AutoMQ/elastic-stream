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

package kv

import (
	"bytes"
	"context"

	"github.com/pkg/errors"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.etcd.io/etcd/server/v3/embed"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/pkg/util/etcdutil"
	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
)

var (
	// ErrTxnFailed is the error when etcd transaction failed.
	ErrTxnFailed = errors.New("etcd transaction failed")
	// ErrTooManyTxnOps is the error when the number of operations in a transaction exceeds the limit.
	ErrTooManyTxnOps = errors.New("too many txn operations")
)

// Etcd is a kv based on etcd.
type Etcd struct {
	rootPath   []byte
	newTxnFunc func(ctx context.Context) clientv3.Txn // WARNING: do not call `If` on the returned txn.
	maxTxnOps  uint

	lg *zap.Logger
}

// EtcdParam is used to create a new etcd kv.
type EtcdParam struct {
	KV clientv3.KV
	// rootPath is the prefix of all keys in etcd.
	RootPath string
	// cmpFunc is used to create a transaction. If cmpFunc is nil, the transaction will not have any condition.
	CmpFunc func() clientv3.Cmp
	// maxTxnOps is the max number of operations in a transaction. It is an etcd server configuration.
	// If maxTxnOps is 0, it will use the default value (128).
	MaxTxnOps uint
}

// NewEtcd creates a new etcd kv.
func NewEtcd(param EtcdParam, lg *zap.Logger) *Etcd {
	e := &Etcd{
		rootPath:  []byte(param.RootPath),
		maxTxnOps: param.MaxTxnOps,
		lg:        lg,
	}

	if e.maxTxnOps == 0 {
		e.maxTxnOps = embed.DefaultMaxTxnOps
	}

	if param.CmpFunc != nil {
		e.newTxnFunc = func(ctx context.Context) clientv3.Txn {
			// cmpFunc should be evaluated lazily.
			return etcdutil.NewTxn(ctx, param.KV, lg.With(traceutil.TraceLogField(ctx))).If(param.CmpFunc())
		}
	} else {
		e.newTxnFunc = func(ctx context.Context) clientv3.Txn {
			return etcdutil.NewTxn(ctx, param.KV, lg.With(traceutil.TraceLogField(ctx)))
		}
	}
	return e
}

// Get returns ErrTxnFailed if EtcdParam.CmpFunc evaluates to false.
func (e *Etcd) Get(ctx context.Context, k []byte) ([]byte, error) {
	if len(k) == 0 {
		return nil, nil
	}

	kvs, err := e.BatchGet(ctx, [][]byte{k}, false)
	if err != nil {
		return nil, errors.Wrap(err, "kv get")
	}

	for _, kv := range kvs {
		if bytes.Equal(kv.Key, k) {
			return kv.Value, nil
		}
	}
	return nil, nil
}

// BatchGet returns ErrTxnFailed if EtcdParam.CmpFunc evaluates to false.
// If inTxn is true, BatchGet returns ErrTooManyTxnOps if the number of keys exceeds EtcdParam.MaxTxnOps.
func (e *Etcd) BatchGet(ctx context.Context, keys [][]byte, inTxn bool) ([]KeyValue, error) {
	if len(keys) == 0 {
		return nil, nil
	}
	batchSize := int(e.maxTxnOps)
	if inTxn && len(keys) > batchSize {
		return nil, errors.Wrap(ErrTooManyTxnOps, "kv batch get")
	}

	kvs := make([]KeyValue, 0, len(keys))

	for i := 0; i < len(keys); i += batchSize {
		end := i + batchSize
		if end > len(keys) {
			end = len(keys)
		}
		batchKeys := keys[i:end]

		ops := make([]clientv3.Op, 0, len(batchKeys))
		for _, k := range batchKeys {
			if len(k) == 0 {
				continue
			}
			key := e.addPrefix(k)
			ops = append(ops, clientv3.OpGet(string(key)))
		}

		txn := e.newTxnFunc(ctx).Then(ops...)
		resp, err := txn.Commit()
		if err != nil {
			return nil, errors.Wrap(err, "kv batch get")
		}
		if !resp.Succeeded {
			return nil, errors.Wrap(ErrTxnFailed, "kv batch get")
		}

		for _, resp := range resp.Responses {
			rangeResp := resp.GetResponseRange()
			if rangeResp == nil {
				continue
			}
			for _, kv := range rangeResp.Kvs {
				if !e.hasPrefix(kv.Key) {
					continue
				}
				kvs = append(kvs, KeyValue{
					Key:   e.trimPrefix(kv.Key),
					Value: kv.Value,
				})
			}
		}
	}

	return kvs, nil
}

// GetByRange returns ErrTxnFailed if EtcdParam.CmpFunc evaluates to false.
func (e *Etcd) GetByRange(ctx context.Context, r Range, limit int64, desc bool) ([]KeyValue, error) {
	if len(r.StartKey) == 0 {
		return nil, nil
	}

	startKey := e.addPrefix(r.StartKey)
	endKey := e.addPrefix(r.EndKey)

	opts := []clientv3.OpOption{clientv3.WithRange(string(endKey))}
	if limit > 0 {
		opts = append(opts, clientv3.WithLimit(limit))
	}
	if desc {
		opts = append(opts, clientv3.WithSort(clientv3.SortByKey, clientv3.SortDescend))
	}

	resp, err := e.newTxnFunc(ctx).Then(clientv3.OpGet(string(startKey), opts...)).Commit()
	if err != nil {
		return nil, errors.Wrap(err, "kv get by range")
	}
	if !resp.Succeeded {
		return nil, errors.Wrap(ErrTxnFailed, "kv get by range")
	}

	cnt := 0
	for _, resp := range resp.Responses {
		rangeResp := resp.GetResponseRange()
		if rangeResp == nil {
			continue
		}
		cnt += len(rangeResp.Kvs)
	}
	kvs := make([]KeyValue, 0, cnt)
	for _, resp := range resp.Responses {
		rangeResp := resp.GetResponseRange()
		if rangeResp == nil {
			continue
		}
		for _, kv := range rangeResp.Kvs {
			if !e.hasPrefix(kv.Key) {
				continue
			}
			kvs = append(kvs, KeyValue{
				Key:   e.trimPrefix(kv.Key),
				Value: kv.Value,
			})
		}
	}
	return kvs, nil
}

// Put returns ErrTxnFailed if EtcdParam.CmpFunc evaluates to false.
func (e *Etcd) Put(ctx context.Context, k, v []byte, prevKV bool) ([]byte, error) {
	if len(k) == 0 {
		return nil, nil
	}

	prevKvs, err := e.BatchPut(ctx, []KeyValue{{Key: k, Value: v}}, prevKV, false)
	if err != nil {
		return nil, errors.Wrap(err, "kv put")
	}

	if !prevKV {
		return nil, nil
	}

	for _, kv := range prevKvs {
		if bytes.Equal(kv.Key, k) {
			return kv.Value, nil
		}
	}
	return nil, nil
}

// BatchPut returns ErrTxnFailed if EtcdParam.CmpFunc evaluates to false.
// If inTxn is true, BatchPut returns ErrTooManyTxnOps if the number of kvs exceeds EtcdParam.MaxTxnOps.
func (e *Etcd) BatchPut(ctx context.Context, kvs []KeyValue, prevKV bool, inTxn bool) ([]KeyValue, error) {
	if len(kvs) == 0 {
		return nil, nil
	}
	batchSize := int(e.maxTxnOps)
	if inTxn && len(kvs) > batchSize {
		return nil, errors.Wrap(ErrTooManyTxnOps, "kv batch put")
	}

	var prevKvs []KeyValue
	if prevKV {
		prevKvs = make([]KeyValue, 0, len(kvs))
	}

	for i := 0; i < len(kvs); i += batchSize {
		end := i + batchSize
		if end > len(kvs) {
			end = len(kvs)
		}
		batchKvs := kvs[i:end]

		ops := make([]clientv3.Op, 0, len(batchKvs))
		var opts []clientv3.OpOption
		if prevKV {
			opts = append(opts, clientv3.WithPrevKV())
		}
		for _, kv := range batchKvs {
			if len(kv.Key) == 0 {
				continue
			}
			key := e.addPrefix(kv.Key)
			ops = append(ops, clientv3.OpPut(string(key), string(kv.Value), opts...))
		}

		txn := e.newTxnFunc(ctx).Then(ops...)
		resp, err := txn.Commit()
		if err != nil {
			return nil, errors.Wrap(err, "kv batch put")
		}
		if !resp.Succeeded {
			return nil, errors.Wrap(ErrTxnFailed, "kv batch put")
		}

		if !prevKV {
			continue
		}
		for _, resp := range resp.Responses {
			putResp := resp.GetResponsePut()
			if putResp.PrevKv == nil {
				continue
			}
			if !e.hasPrefix(putResp.PrevKv.Key) {
				continue
			}
			prevKvs = append(prevKvs, KeyValue{
				Key:   e.trimPrefix(putResp.PrevKv.Key),
				Value: putResp.PrevKv.Value,
			})
		}
	}

	return prevKvs, nil
}

// Delete returns ErrTxnFailed if EtcdParam.CmpFunc evaluates to false.
func (e *Etcd) Delete(ctx context.Context, k []byte, prevKV bool) ([]byte, error) {
	if len(k) == 0 {
		return nil, nil
	}

	prevKvs, err := e.BatchDelete(ctx, [][]byte{k}, prevKV, false)
	if err != nil {
		return nil, errors.Wrap(err, "kv delete")
	}

	if !prevKV {
		return nil, nil
	}
	for _, kv := range prevKvs {
		if bytes.Equal(kv.Key, k) {
			return kv.Value, nil
		}
	}
	return nil, nil
}

// BatchDelete returns ErrTxnFailed if EtcdParam.CmpFunc evaluates to false.
// If inTxn is true, BatchDelete returns ErrTooManyTxnOps if the number of keys exceeds EtcdParam.MaxTxnOps.
func (e *Etcd) BatchDelete(ctx context.Context, keys [][]byte, prevKV bool, inTxn bool) ([]KeyValue, error) {
	if len(keys) == 0 {
		return nil, nil
	}
	batchSize := int(e.maxTxnOps)
	if inTxn && len(keys) > batchSize {
		return nil, errors.Wrap(ErrTooManyTxnOps, "kv batch delete")
	}

	var prevKvs []KeyValue
	if prevKV {
		prevKvs = make([]KeyValue, 0, len(keys))
	}

	for i := 0; i < len(keys); i += batchSize {
		end := i + batchSize
		if end > len(keys) {
			end = len(keys)
		}
		batchKeys := keys[i:end]

		ops := make([]clientv3.Op, 0, len(batchKeys))
		var opts []clientv3.OpOption
		if prevKV {
			opts = append(opts, clientv3.WithPrevKV())
		}
		for _, k := range batchKeys {
			if len(k) == 0 {
				continue
			}
			key := e.addPrefix(k)
			ops = append(ops, clientv3.OpDelete(string(key), opts...))
		}

		txn := e.newTxnFunc(ctx).Then(ops...)
		resp, err := txn.Commit()
		if err != nil {
			return nil, errors.Wrap(err, "kv batch delete")
		}
		if !resp.Succeeded {
			return nil, errors.Wrap(ErrTxnFailed, "kv batch delete")
		}

		if !prevKV {
			continue
		}
		for _, resp := range resp.Responses {
			deleteResp := resp.GetResponseDeleteRange()
			for _, kv := range deleteResp.PrevKvs {
				if !e.hasPrefix(kv.Key) {
					continue
				}
				prevKvs = append(prevKvs, KeyValue{
					Key:   e.trimPrefix(kv.Key),
					Value: kv.Value,
				})
			}
		}
	}

	return prevKvs, nil
}

func (e *Etcd) GetPrefixRangeEnd(p []byte) []byte {
	prefix := e.addPrefix(p)
	end := []byte(clientv3.GetPrefixRangeEnd(string(prefix)))
	return e.trimPrefix(end)
}

func (e *Etcd) Logger() *zap.Logger {
	return e.lg
}

func (e *Etcd) addPrefix(k []byte) []byte {
	return bytes.Join([][]byte{e.rootPath, k}, []byte(KeySeparator))
}

func (e *Etcd) trimPrefix(k []byte) []byte {
	return k[len(e.rootPath)+len(KeySeparator):]
}

func (e *Etcd) hasPrefix(k []byte) bool {
	return len(k) >= len(e.rootPath)+len(KeySeparator) &&
		bytes.Equal(k[:len(e.rootPath)], e.rootPath) &&
		string(k[len(e.rootPath):len(e.rootPath)+len(KeySeparator)]) == KeySeparator
}
