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
	_rangeIDFormat = _int32Format
	_rangeIDLen    = _int32Len

	// ranges in stream
	_rangeStreamPath         = "ranges"
	_rangeStreamPrefix       = "s"
	_rangeStreamPrefixFormat = _rangeStreamPrefix + kv.KeySeparator + _streamIDFormat + kv.KeySeparator + _rangeStreamPath + kv.KeySeparator
	_rangeStreamFormat       = _rangeStreamPrefix + kv.KeySeparator + _streamIDFormat + kv.KeySeparator + _rangeStreamPath + kv.KeySeparator + _rangeIDFormat
	_rangeStreamKeyLen       = len(_rangeStreamPrefix) + len(kv.KeySeparator) + _streamIDLen + len(kv.KeySeparator) + len(_rangeStreamPath) + len(kv.KeySeparator) + _rangeIDLen

	// ranges on range server
	_rangeOnServerPath               = "stream-range"
	_rangeOnServerPrefix             = "rs"
	_rangeOnServerPrefixFormat       = _rangeOnServerPrefix + kv.KeySeparator + _rangeServerIDFormat + kv.KeySeparator + _rangeOnServerPath + kv.KeySeparator
	_rangeOnServerStreamPrefixFormat = _rangeOnServerPrefix + kv.KeySeparator + _rangeServerIDFormat + kv.KeySeparator + _rangeOnServerPath + kv.KeySeparator + _streamIDFormat + kv.KeySeparator
	_rangeOnServerFormat             = _rangeOnServerPrefix + kv.KeySeparator + _rangeServerIDFormat + kv.KeySeparator + _rangeOnServerPath + kv.KeySeparator + _streamIDFormat + kv.KeySeparator + _rangeIDFormat
	_rangeOnServerKeyLen             = len(_rangeOnServerPrefix) + len(kv.KeySeparator) + _rangeServerIDLen + len(kv.KeySeparator) + len(_rangeOnServerPath) + len(kv.KeySeparator) + _streamIDLen + len(kv.KeySeparator) + _rangeIDLen

	_rangeByRangeLimit = 1e4
)

type RangeID struct {
	StreamID int64
	Index    int32
}

type Range interface {
	CreateRange(ctx context.Context, rangeT *rpcfb.RangeT) error
	UpdateRange(ctx context.Context, rangeT *rpcfb.RangeT) (*rpcfb.RangeT, error)
	GetRange(ctx context.Context, rangeID *RangeID) (*rpcfb.RangeT, error)
	GetRanges(ctx context.Context, rangeIDs []*RangeID) ([]*rpcfb.RangeT, error)
	GetLastRange(ctx context.Context, streamID int64) (*rpcfb.RangeT, error)
	GetRangesByStream(ctx context.Context, streamID int64) ([]*rpcfb.RangeT, error)
	ForEachRangeInStream(ctx context.Context, streamID int64, f func(r *rpcfb.RangeT) error) error
	GetRangeIDsByRangeServer(ctx context.Context, rangeServerID int32) ([]*RangeID, error)
	ForEachRangeIDOnRangeServer(ctx context.Context, rangeServerID int32, f func(rangeID *RangeID) error) error
	GetRangeIDsByRangeServerAndStream(ctx context.Context, streamID int64, rangeServerID int32) ([]*RangeID, error)
}

func (e *Endpoint) CreateRange(ctx context.Context, rangeT *rpcfb.RangeT) error {
	logger := e.lg.With(zap.Int64("stream-id", rangeT.StreamId), zap.Int32("range-index", rangeT.Index), traceutil.TraceLogField(ctx))

	kvs := make([]kv.KeyValue, 0, 1+len(rangeT.Servers))
	r := fbutil.Marshal(rangeT)
	kvs = append(kvs, kv.KeyValue{
		Key:   rangePathInSteam(rangeT.StreamId, rangeT.Index),
		Value: r,
	})
	for _, server := range rangeT.Servers {
		kvs = append(kvs, kv.KeyValue{
			Key:   rangePathOnRangeServer(server.ServerId, rangeT.StreamId, rangeT.Index),
			Value: nil,
		})
	}

	prevKVs, err := e.BatchPut(ctx, kvs, true, true)
	mcache.Free(r)

	if err != nil {
		logger.Error("failed to create range", zap.Error(err))
		return errors.Wrap(err, "create range")
	}
	if len(prevKVs) != 0 {
		logger.Warn("range already exists when create range")
		return nil
	}

	return nil
}

// UpdateRange updates the range and returns the previous range.
func (e *Endpoint) UpdateRange(ctx context.Context, rangeT *rpcfb.RangeT) (*rpcfb.RangeT, error) {
	logger := e.lg.With(zap.Int64("stream-id", rangeT.StreamId), zap.Int32("range-index", rangeT.Index), traceutil.TraceLogField(ctx))

	key := rangePathInSteam(rangeT.StreamId, rangeT.Index)
	value := fbutil.Marshal(rangeT)

	prevValue, err := e.Put(ctx, key, value, true)
	mcache.Free(value)
	if err != nil {
		logger.Error("failed to update range", zap.Error(err))
		return nil, errors.Wrap(err, "update range")
	}
	if prevValue == nil {
		logger.Warn("range not found when update range")
		return nil, nil
	}

	return rpcfb.GetRootAsRange(prevValue, 0).UnPack(), nil
}

func (e *Endpoint) GetRange(ctx context.Context, rangeID *RangeID) (*rpcfb.RangeT, error) {
	ranges, err := e.GetRanges(ctx, []*RangeID{rangeID})
	if err != nil {
		return nil, err
	}
	if len(ranges) == 0 {
		return nil, nil
	}

	return ranges[0], nil
}

func (e *Endpoint) GetRanges(ctx context.Context, rangeIDs []*RangeID) ([]*rpcfb.RangeT, error) {
	logger := e.lg.With(traceutil.TraceLogField(ctx))

	keys := make([][]byte, len(rangeIDs))
	for i, rangeID := range rangeIDs {
		keys[i] = rangePathInSteam(rangeID.StreamID, rangeID.Index)
	}

	kvs, err := e.BatchGet(ctx, keys, false)
	if err != nil {
		logger.Error("failed to get ranges", zap.Error(err))
		return nil, errors.Wrap(err, "get ranges")
	}

	ranges := make([]*rpcfb.RangeT, 0, len(kvs))
	for _, rangeKV := range kvs {
		if rangeKV.Value == nil {
			continue
		}
		ranges = append(ranges, rpcfb.GetRootAsRange(rangeKV.Value, 0).UnPack())
	}

	if len(ranges) < len(rangeIDs) {
		logger.Warn("some ranges not found", zap.Int("expected-count", len(rangeIDs)), zap.Int("got-count", len(ranges)),
			zap.Any("expected", rangeIDs), zap.Any("got", rangeIDsFromPathsInStream(kvs)))
	}

	return ranges, nil
}

func (e *Endpoint) GetLastRange(ctx context.Context, streamID int64) (*rpcfb.RangeT, error) {
	logger := e.lg.With(zap.Int64("stream-id", streamID), traceutil.TraceLogField(ctx))

	kvs, _, err := e.GetByRange(ctx, kv.Range{StartKey: rangePathInSteam(streamID, MinRangeIndex), EndKey: e.endRangePathInStream(streamID)}, 1, true)
	if err != nil {
		logger.Error("failed to get last range", zap.Error(err))
		return nil, err
	}
	if len(kvs) < 1 {
		logger.Info("no range in stream")
		return nil, nil
	}

	return rpcfb.GetRootAsRange(kvs[0].Value, 0).UnPack(), nil
}

// GetRangesByStream returns the ranges of the given stream.
func (e *Endpoint) GetRangesByStream(ctx context.Context, streamID int64) ([]*rpcfb.RangeT, error) {
	logger := e.lg.With(zap.Int64("stream-id", streamID), traceutil.TraceLogField(ctx))

	// TODO set capacity
	ranges := make([]*rpcfb.RangeT, 0)
	err := e.ForEachRangeInStream(ctx, streamID, func(r *rpcfb.RangeT) error {
		ranges = append(ranges, r)
		return nil
	})
	if err != nil {
		logger.Error("failed to get ranges", zap.Error(err))
		return nil, errors.Wrap(err, "get ranges")
	}

	return ranges, nil
}

// ForEachRangeInStream calls the given function f for each range in the stream.
// If f returns an error, the iteration is stopped and the error is returned.
func (e *Endpoint) ForEachRangeInStream(ctx context.Context, streamID int64, f func(r *rpcfb.RangeT) error) error {
	var startID = MinRangeIndex
	for startID >= MinRangeIndex {
		nextID, err := e.forEachRangeInStreamLimited(ctx, streamID, f, startID, _rangeByRangeLimit)
		if err != nil {
			return err
		}
		startID = nextID
	}
	return nil
}

func (e *Endpoint) forEachRangeInStreamLimited(ctx context.Context, streamID int64, f func(r *rpcfb.RangeT) error, startID int32, limit int64) (nextID int32, err error) {
	logger := e.lg.With(zap.Int64("stream-id", streamID), traceutil.TraceLogField(ctx))

	startKey := rangePathInSteam(streamID, startID)
	kvs, more, err := e.GetByRange(ctx, kv.Range{StartKey: startKey, EndKey: e.endRangePathInStream(streamID)}, limit, false)
	if err != nil {
		logger.Error("failed to get ranges", zap.Int32("start-id", startID), zap.Int64("limit", limit), zap.Error(err))
		return MinRangeIndex - 1, errors.Wrap(err, "get ranges")
	}

	for _, rangeKV := range kvs {
		r := rpcfb.GetRootAsRange(rangeKV.Value, 0).UnPack()
		nextID = r.Index + 1
		err = f(r)
		if err != nil {
			return MinRangeIndex - 1, err
		}
	}

	if !more {
		// no more ranges
		nextID = MinRangeIndex - 1
	}
	return
}

func (e *Endpoint) endRangePathInStream(streamID int64) []byte {
	return e.GetPrefixRangeEnd([]byte(fmt.Sprintf(_rangeStreamPrefixFormat, streamID)))
}

func rangePathInSteam(streamID int64, index int32) []byte {
	res := make([]byte, 0, _rangeStreamKeyLen)
	res = fmt.Appendf(res, _rangeStreamFormat, streamID, index)
	return res
}

func rangeIDsFromPathsInStream(kvs []kv.KeyValue) []*RangeID {
	rangeIDs := make([]*RangeID, 0, len(kvs))
	for _, rangeKV := range kvs {
		streamID, index, err := rangeIDFromPathInStream(rangeKV.Key)
		if err != nil {
			continue
		}
		rangeIDs = append(rangeIDs, &RangeID{StreamID: streamID, Index: index})
	}
	return rangeIDs
}

func rangeIDFromPathInStream(path []byte) (streamID int64, index int32, err error) {
	_, err = fmt.Sscanf(string(path), _rangeStreamFormat, &streamID, &index)
	if err != nil {
		err = errors.Wrapf(err, "invalid range path %s", string(path))
	}
	return
}

func (e *Endpoint) GetRangeIDsByRangeServer(ctx context.Context, rangeServerID int32) ([]*RangeID, error) {
	logger := e.lg.With(zap.Int32("range-server-id", rangeServerID), traceutil.TraceLogField(ctx))

	// TODO set capacity
	rangeIDs := make([]*RangeID, 0)
	err := e.ForEachRangeIDOnRangeServer(ctx, rangeServerID, func(rangeID *RangeID) error {
		rangeIDs = append(rangeIDs, rangeID)
		return nil
	})
	if err != nil {
		logger.Error("failed to get range ids by range server", zap.Error(err))
		return nil, errors.Wrap(err, "get range ids by range server")
	}

	return rangeIDs, nil
}

// ForEachRangeIDOnRangeServer calls the given function f for each range on the range server.
// If f returns an error, the iteration is stopped and the error is returned.
func (e *Endpoint) ForEachRangeIDOnRangeServer(ctx context.Context, rangeServerID int32, f func(rangeID *RangeID) error) error {
	startID := &RangeID{StreamID: MinStreamID, Index: MinRangeIndex}
	for startID != nil && startID.StreamID >= MinStreamID && startID.Index >= MinRangeIndex {
		nextID, err := e.forEachRangeIDOnRangeServerLimited(ctx, rangeServerID, f, startID, _rangeByRangeLimit)
		if err != nil {
			return err
		}
		startID = nextID
	}
	return nil
}

func (e *Endpoint) forEachRangeIDOnRangeServerLimited(ctx context.Context, rangeServerID int32, f func(rangeID *RangeID) error, startID *RangeID, limit int64) (nextID *RangeID, err error) {
	logger := e.lg.With(zap.Int32("range-server-id", rangeServerID), traceutil.TraceLogField(ctx))

	startKey := rangePathOnRangeServer(rangeServerID, startID.StreamID, startID.Index)
	kvs, more, err := e.GetByRange(ctx, kv.Range{StartKey: startKey, EndKey: e.endRangePathOnRangeServer(rangeServerID)}, limit, false)
	if err != nil {
		logger.Error("failed to get range ids by range server", zap.Int64("start-stream-id", startID.StreamID), zap.Int32("start-range-index", startID.Index), zap.Int64("limit", limit), zap.Error(err))
		return nil, errors.Wrap(err, "get range ids by range server")
	}

	nextID = startID
	for _, rangeKV := range kvs {
		_, streamID, index, err := rangeIDFromPathOnRangeServer(rangeKV.Key)
		if err != nil {
			return nil, err
		}
		nextID.StreamID = streamID
		nextID.Index = index + 1
		err = f(&RangeID{StreamID: streamID, Index: index})
		if err != nil {
			return nil, err
		}
	}

	// return nil if no more ranges
	if !more {
		nextID = nil
	}
	return
}

func (e *Endpoint) endRangePathOnRangeServer(rangeServerID int32) []byte {
	return e.GetPrefixRangeEnd([]byte(fmt.Sprintf(_rangeOnServerPrefixFormat, rangeServerID)))
}

func (e *Endpoint) GetRangeIDsByRangeServerAndStream(ctx context.Context, streamID int64, rangeServerID int32) ([]*RangeID, error) {
	logger := e.lg.With(zap.Int64("stream-id", streamID), zap.Int32("range-server-id", rangeServerID), traceutil.TraceLogField(ctx))

	startKey := rangePathOnRangeServer(rangeServerID, streamID, MinRangeIndex)
	kvs, _, err := e.GetByRange(ctx, kv.Range{StartKey: startKey, EndKey: e.endRangePathOnRangeServerInStream(rangeServerID, streamID)}, 0, false)
	if err != nil {
		logger.Error("failed to get range ids by range server and stream", zap.Error(err))
		return nil, errors.Wrap(err, "get range ids by range server and stream")
	}

	rangeIDs := make([]*RangeID, 0, len(kvs))
	for _, rangeKV := range kvs {
		_, _, index, err := rangeIDFromPathOnRangeServer(rangeKV.Key)
		if err != nil {
			return nil, err
		}
		rangeIDs = append(rangeIDs, &RangeID{StreamID: streamID, Index: index})
	}

	return rangeIDs, nil
}

func (e *Endpoint) endRangePathOnRangeServerInStream(rangeServerID int32, streamID int64) []byte {
	return e.GetPrefixRangeEnd([]byte(fmt.Sprintf(_rangeOnServerStreamPrefixFormat, rangeServerID, streamID)))
}

func rangePathOnRangeServer(rangeServerID int32, streamID int64, index int32) []byte {
	res := make([]byte, 0, _rangeOnServerKeyLen)
	res = fmt.Appendf(res, _rangeOnServerFormat, rangeServerID, streamID, index)
	return res
}

func rangeIDFromPathOnRangeServer(path []byte) (rangeServerID int32, streamID int64, index int32, err error) {
	_, err = fmt.Sscanf(string(path), _rangeOnServerFormat, &rangeServerID, &streamID, &index)
	if err != nil {
		err = errors.Wrapf(err, "parse range path: %s", string(path))
	}
	return
}
