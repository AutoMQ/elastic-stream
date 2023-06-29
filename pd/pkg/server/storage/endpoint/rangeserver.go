package endpoint

import (
	"context"
	"fmt"

	"github.com/bytedance/gopkg/lang/mcache"
	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/server/storage/kv"
	"github.com/AutoMQ/pd/pkg/util/fbutil"
	"github.com/AutoMQ/pd/pkg/util/traceutil"
)

const (
	_rangeServerIDFormat = _int32Format
	_rangeServerIDLen    = _int32Len

	_rangeServerPath   = "range-servers"
	_rangeServerPrefix = _rangeServerPath + kv.KeySeparator
	_rangeServerFormat = _rangeServerPath + kv.KeySeparator + _rangeServerIDFormat
	_rangeServerKeyLen = len(_rangeServerPath) + len(kv.KeySeparator) + _rangeServerIDLen

	_rangeServerByRangeLimit = 1e4
)

type RangeServer interface {
	SaveRangeServer(ctx context.Context, rangeServer *rpcfb.RangeServerT) (*rpcfb.RangeServerT, error)
	ForEachRangeServer(ctx context.Context, f func(rangeServer *rpcfb.RangeServerT) error) error
}

// SaveRangeServer creates or updates the given range server and returns it.
func (e *Endpoint) SaveRangeServer(ctx context.Context, rangeServer *rpcfb.RangeServerT) (*rpcfb.RangeServerT, error) {
	logger := e.lg.With(zap.Int32("server-id", rangeServer.ServerId), traceutil.TraceLogField(ctx))

	if rangeServer.ServerId < MinRangeServerID {
		logger.Error("invalid range server id")
		return nil, errors.Errorf("invalid range server id: %d < %d", rangeServer.ServerId, MinRangeServerID)
	}

	key := rangeServerPath(rangeServer.ServerId)
	value := fbutil.Marshal(rangeServer)
	defer mcache.Free(value)

	_, err := e.Put(ctx, key, value, false)
	if err != nil {
		logger.Error("failed to save range server", zap.Error(err))
		return nil, errors.Wrap(err, "save range server")
	}

	return rangeServer, nil
}

// ForEachRangeServer calls the given function for every range server in the storage.
// If f returns an error, the iteration is stopped and the error is returned.
func (e *Endpoint) ForEachRangeServer(ctx context.Context, f func(rangeServer *rpcfb.RangeServerT) error) error {
	var startID = MinRangeServerID
	for startID >= MinRangeServerID {
		nextID, err := e.forEachRangeServerLimited(ctx, f, startID, _rangeServerByRangeLimit)
		if err != nil {
			return err
		}
		startID = nextID
	}
	return nil
}

func (e *Endpoint) forEachRangeServerLimited(ctx context.Context, f func(rangeServer *rpcfb.RangeServerT) error, startID int32, limit int64) (nextID int32, err error) {
	logger := e.lg.With(traceutil.TraceLogField(ctx))

	startKey := rangeServerPath(startID)
	kvs, err := e.GetByRange(ctx, kv.Range{StartKey: startKey, EndKey: e.endRangeServerPath()}, limit, false)
	if err != nil {
		logger.Error("failed to get range servers", zap.Int32("start-id", startID), zap.Int64("limit", limit), zap.Error(err))
		return MinRangeServerID - 1, errors.Wrap(err, "get range servers")
	}

	for _, keyValue := range kvs {
		rangeServer := rpcfb.GetRootAsRangeServer(keyValue.Value, 0).UnPack()
		nextID = rangeServer.ServerId + 1
		err = f(rangeServer)
		if err != nil {
			return MinRangeServerID - 1, err
		}
	}

	if int64(len(kvs)) < limit {
		// no more range servers
		nextID = MinRangeServerID - 1
	}
	return
}

func (e *Endpoint) endRangeServerPath() []byte {
	return e.GetPrefixRangeEnd([]byte(_rangeServerPrefix))
}

func rangeServerPath(serverID int32) []byte {
	res := make([]byte, 0, _rangeServerKeyLen)
	res = fmt.Appendf(res, _rangeServerFormat, serverID)
	return res
}
