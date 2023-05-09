package kv

import (
	"context"
	"fmt"

	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
)

type LogAble interface {
	KV
	Logger() *zap.Logger
}

// Logger is a wrapper of KV that logs all operations.
type Logger struct {
	Kv LogAble
}

func (l Logger) Get(ctx context.Context, k []byte) (v []byte, err error) {
	v, err = l.Kv.Get(ctx, k)

	logger := l.logger()
	if logger.Core().Enabled(zap.DebugLevel) {
		logger = logger.With(traceutil.TraceLogField(ctx))
		logger.Debug("kv get", zap.ByteString("key", k), zap.Binary("value", v), zap.Error(err))
	}
	return
}

func (l Logger) BatchGet(ctx context.Context, keys [][]byte) (kvs []KeyValue, err error) {
	kvs, err = l.Kv.BatchGet(ctx, keys)

	logger := l.logger()
	if logger.Core().Enabled(zap.DebugLevel) {
		logger = logger.With(traceutil.TraceLogField(ctx))
		fields := []zap.Field{
			zap.Error(err),
		}
		for i, key := range keys {
			fields = append(fields, zap.ByteString(fmt.Sprintf("query-key-%d", i), key))
		}
		for i, kv := range kvs {
			fields = append(fields, zap.ByteString(fmt.Sprintf("key-%d", i), kv.Key), zap.Binary(fmt.Sprintf("value-%d", i), kv.Value))
		}
		logger.Debug("kv batch get", fields...)
	}
	return
}

func (l Logger) GetByRange(ctx context.Context, r Range, limit int64, desc bool) (kvs []KeyValue, err error) {
	kvs, err = l.Kv.GetByRange(ctx, r, limit, desc)

	logger := l.logger()
	if logger.Core().Enabled(zap.DebugLevel) {
		logger = logger.With(traceutil.TraceLogField(ctx))
		fields := []zap.Field{
			zap.ByteString("start-key", r.StartKey),
			zap.ByteString("end-key", r.EndKey),
			zap.Int64("limit", limit),
			zap.Error(err),
		}
		for i, kv := range kvs {
			fields = append(fields, zap.ByteString(fmt.Sprintf("key-%d", i), kv.Key), zap.Binary(fmt.Sprintf("value-%d", i), kv.Value))
		}
		logger.Debug("kv get by range", fields...)
	}
	return
}

func (l Logger) Put(ctx context.Context, k, v []byte, prevKV bool) (prevV []byte, err error) {
	prevV, err = l.Kv.Put(ctx, k, v, prevKV)

	logger := l.logger()
	if logger.Core().Enabled(zap.DebugLevel) {
		logger = logger.With(traceutil.TraceLogField(ctx))
		logger.Debug("kv put", zap.ByteString("key", k), zap.Binary("value", v), zap.Bool("prev-kv", prevKV), zap.Binary("prev-value", prevV), zap.Error(err))
	}
	return
}

func (l Logger) BatchPut(ctx context.Context, kvs []KeyValue, prevKV bool) (prevKvs []KeyValue, err error) {
	prevKvs, err = l.Kv.BatchPut(ctx, kvs, prevKV)

	logger := l.logger()
	if logger.Core().Enabled(zap.DebugLevel) {
		logger = logger.With(traceutil.TraceLogField(ctx))
		fields := []zap.Field{
			zap.Bool("prev-kv", prevKV),
			zap.Error(err),
		}
		for i, kv := range kvs {
			fields = append(fields, zap.ByteString(fmt.Sprintf("key-%d", i), kv.Key), zap.Binary(fmt.Sprintf("value-%d", i), kv.Value))
		}
		for i, kv := range prevKvs {
			fields = append(fields, zap.ByteString(fmt.Sprintf("prev-key-%d", i), kv.Key), zap.Binary(fmt.Sprintf("prev-value-%d", i), kv.Value))
		}
		logger.Debug("kv batch put", fields...)
	}
	return
}

func (l Logger) Delete(ctx context.Context, k []byte, prevKV bool) (prevV []byte, err error) {
	prevV, err = l.Kv.Delete(ctx, k, prevKV)

	logger := l.logger()
	if logger.Core().Enabled(zap.DebugLevel) {
		logger = logger.With(traceutil.TraceLogField(ctx))
		logger.Debug("kv delete", zap.ByteString("key", k), zap.Bool("prev-kv", prevKV), zap.Binary("prev-value", prevV), zap.Error(err))
	}
	return
}

func (l Logger) BatchDelete(ctx context.Context, ks [][]byte, prevKV bool) (prevKvs []KeyValue, err error) {
	prevKvs, err = l.Kv.BatchDelete(ctx, ks, prevKV)

	logger := l.logger()
	if logger.Core().Enabled(zap.DebugLevel) {
		logger = logger.With(traceutil.TraceLogField(ctx))
		fields := []zap.Field{
			zap.Bool("prev-kv", prevKV),
			zap.Error(err),
		}
		for i, k := range ks {
			fields = append(fields, zap.ByteString(fmt.Sprintf("key-%d", i), k))
		}
		for i, kv := range prevKvs {
			fields = append(fields, zap.ByteString(fmt.Sprintf("prev-key-%d", i), kv.Key), zap.Binary(fmt.Sprintf("prev-value-%d", i), kv.Value))
		}
		logger.Debug("kv batch delete", fields...)
	}
	return
}

func (l Logger) GetPrefixRangeEnd(prefix []byte) []byte {
	return l.Kv.GetPrefixRangeEnd(prefix)
}

func (l Logger) logger() *zap.Logger {
	if l.Kv.Logger() != nil {
		return l.Kv.Logger()
	}
	return zap.NewNop()
}
