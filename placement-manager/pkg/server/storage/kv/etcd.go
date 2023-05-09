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
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/pkg/util/etcdutil"
	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
)

var (
	// ErrTxnFailed is the error when etcd transaction failed.
	ErrTxnFailed = errors.New("etcd transaction failed")
)

// Etcd is a kv based on etcd.
type Etcd struct {
	client     *clientv3.Client
	rootPath   []byte
	newTxnFunc func(ctx context.Context) clientv3.Txn // WARNING: do not call `If` on the returned txn.

	lg *zap.Logger
}

// NewEtcd creates a new etcd kv.
// The rootPath is the prefix of all keys in etcd.
// The cmpFunc is used to create a transaction.
// If cmpFunc is nil, the transaction will not have any condition.
func NewEtcd(client *clientv3.Client, rootPath string, lg *zap.Logger, cmpFunc func() clientv3.Cmp) *Etcd {
	e := &Etcd{
		client:   client,
		rootPath: []byte(rootPath),
		lg:       lg,
	}
	if cmpFunc != nil {
		e.newTxnFunc = func(ctx context.Context) clientv3.Txn {
			// cmpFunc should be evaluated lazily.
			return etcdutil.NewTxn(ctx, client, lg.With(traceutil.TraceLogField(ctx))).If(cmpFunc())
		}
	} else {
		e.newTxnFunc = func(ctx context.Context) clientv3.Txn {
			return etcdutil.NewTxn(ctx, client, lg.With(traceutil.TraceLogField(ctx)))
		}
	}
	return e
}

func (e *Etcd) Get(ctx context.Context, k []byte) ([]byte, error) {
	if len(k) == 0 {
		return nil, nil
	}

	kvs, err := e.BatchGet(ctx, [][]byte{k})
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

func (e *Etcd) BatchGet(ctx context.Context, keys [][]byte) ([]KeyValue, error) {
	if len(keys) == 0 {
		return nil, nil
	}

	ops := make([]clientv3.Op, 0, len(keys))
	for _, k := range keys {
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

	kvs := make([]KeyValue, 0, len(resp.Responses))
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

func (e *Etcd) GetByRange(ctx context.Context, r Range, limit int64, desc bool) ([]KeyValue, error) {
	if len(r.StartKey) == 0 {
		return nil, nil
	}
	logger := e.lg

	startKey := e.addPrefix(r.StartKey)
	endKey := e.addPrefix(r.EndKey)

	opts := []clientv3.OpOption{clientv3.WithRange(string(endKey))}
	if limit > 0 {
		opts = append(opts, clientv3.WithLimit(limit))
	}
	if desc {
		opts = append(opts, clientv3.WithSort(clientv3.SortByKey, clientv3.SortDescend))
	}
	resp, err := etcdutil.Get(ctx, e.client, startKey, logger, opts...)
	if err != nil {
		return nil, errors.Wrap(err, "kv get by range")
	}

	kvs := make([]KeyValue, 0, len(resp.Kvs))
	for _, kv := range resp.Kvs {
		if !e.hasPrefix(kv.Key) {
			continue
		}
		kvs = append(kvs, KeyValue{
			Key:   e.trimPrefix(kv.Key),
			Value: kv.Value,
		})
	}
	return kvs, nil
}

func (e *Etcd) Put(ctx context.Context, k, v []byte, prevKV bool) ([]byte, error) {
	if len(k) == 0 {
		return nil, nil
	}

	prevKvs, err := e.BatchPut(ctx, []KeyValue{{Key: k, Value: v}}, prevKV)
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

// BatchPut returns ErrTxnFailed if transaction failed.
func (e *Etcd) BatchPut(ctx context.Context, kvs []KeyValue, prevKV bool) ([]KeyValue, error) {
	if len(kvs) == 0 {
		return nil, nil
	}

	ops := make([]clientv3.Op, 0, len(kvs))
	var opts []clientv3.OpOption
	if prevKV {
		opts = append(opts, clientv3.WithPrevKV())
	}
	for _, kv := range kvs {
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
		return nil, nil
	}
	prevKvs := make([]KeyValue, 0, len(resp.Responses))
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
	return prevKvs, nil
}

func (e *Etcd) Delete(ctx context.Context, k []byte, prevKV bool) ([]byte, error) {
	if len(k) == 0 {
		return nil, nil
	}

	prevKvs, err := e.BatchDelete(ctx, [][]byte{k}, prevKV)
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

// BatchDelete returns ErrTxnFailed if transaction failed.
func (e *Etcd) BatchDelete(ctx context.Context, keys [][]byte, prevKV bool) ([]KeyValue, error) {
	if len(keys) == 0 {
		return nil, nil
	}

	ops := make([]clientv3.Op, 0, len(keys))
	var opts []clientv3.OpOption
	if prevKV {
		opts = append(opts, clientv3.WithPrevKV())
	}
	for _, k := range keys {
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
		return nil, nil
	}
	prevKvs := make([]KeyValue, 0, len(resp.Responses))
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
