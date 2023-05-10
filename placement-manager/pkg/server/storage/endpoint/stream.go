package endpoint

import (
	"context"
	"fmt"

	"github.com/bytedance/gopkg/lang/mcache"
	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/server/storage/kv"
	"github.com/AutoMQ/placement-manager/pkg/util/fbutil"
	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
)

const (
	_streamIDFormat = _int64Format
	_streamIDLen    = _int64Len

	_streamPath   = "stream"
	_streamPrefix = _streamPath + kv.KeySeparator
	_streamFormat = _streamPath + kv.KeySeparator + _streamIDFormat // max length of int64 is 20
	_streamKeyLen = len(_streamPath) + len(kv.KeySeparator) + _streamIDLen

	_streamByRangeLimit = 1e4
)

// CreateStreamParam defines the parameters of creating a stream.
type CreateStreamParam struct {
	*rpcfb.StreamT
	// TODO remove it
	*rpcfb.RangeT
}

// Stream defines operations on stream.
type Stream interface {
	CreateStreams(ctx context.Context, params []*CreateStreamParam) ([]*rpcfb.StreamT, error)
	DeleteStreams(ctx context.Context, streamIDs []int64) ([]*rpcfb.StreamT, error)
	UpdateStreams(ctx context.Context, streams []*rpcfb.StreamT) ([]*rpcfb.StreamT, error)
	GetStream(ctx context.Context, streamID int64) (*rpcfb.StreamT, error)
	GetStreams(ctx context.Context, streamIDs []int64) ([]*rpcfb.StreamT, error)
	ForEachStream(ctx context.Context, f func(stream *rpcfb.StreamT) error) error
}

// CreateStreams creates new streams based on the given streams and returns them.
func (e *Endpoint) CreateStreams(ctx context.Context, params []*CreateStreamParam) ([]*rpcfb.StreamT, error) {
	logger := e.lg.With(traceutil.TraceLogField(ctx))

	results := make([]*rpcfb.StreamT, 0, len(params))
	kvs := make([]kv.KeyValue, 0, len(params)*5)
	for _, param := range params {
		kvs = append(kvs, kv.KeyValue{
			Key:   streamPath(param.StreamT.StreamId),
			Value: fbutil.Marshal(param.StreamT),
		})
		kvs = append(kvs, kv.KeyValue{
			Key:   rangePathInSteam(param.StreamT.StreamId, param.RangeT.RangeIndex),
			Value: fbutil.Marshal(param.RangeT),
		})
		for _, node := range param.RangeT.ReplicaNodes {
			kvs = append(kvs, kv.KeyValue{
				Key:   rangePathOnDataNode(node.NodeId, param.StreamT.StreamId, param.RangeT.RangeIndex),
				Value: nil,
			})
		}
		results = append(results, param.StreamT)
	}

	prevKvs, err := e.BatchPut(ctx, kvs, true)
	for _, keyValue := range kvs {
		if keyValue.Value != nil {
			mcache.Free(keyValue.Value)
		}
	}
	if err != nil {
		streamIDs := make([]int64, 0, len(params))
		for _, param := range params {
			streamIDs = append(streamIDs, param.StreamT.StreamId)
		}
		logger.Error("failed to save streams", zap.Int64s("stream-ids", streamIDs), zap.Error(err))
		return nil, errors.Wrap(err, "save streams")
	}
	if len(prevKvs) != 0 {
		existedStreamIDs := streamIDsFromPaths(prevKvs)
		logger.Warn("streams already exist, will override them", zap.Int64s("existed-stream-ids", existedStreamIDs))
	}

	return results, nil
}

// DeleteStreams deletes the streams with the given stream ids and returns them.
func (e *Endpoint) DeleteStreams(ctx context.Context, streamIDs []int64) ([]*rpcfb.StreamT, error) {
	logger := e.lg.With(zap.Int64s("stream-ids", streamIDs), traceutil.TraceLogField(ctx))

	streamPaths := make([][]byte, 0, len(streamIDs))
	for _, streamID := range streamIDs {
		streamPaths = append(streamPaths, streamPath(streamID))
	}
	prevKvs, err := e.BatchDelete(ctx, streamPaths, true)
	if err != nil {
		logger.Error("failed to delete stream", zap.Error(err))
		return nil, errors.Wrap(err, "delete stream")
	}
	if len(prevKvs) < len(streamIDs) {
		existedStreamIDs := streamIDsFromPaths(prevKvs)
		logger.Warn("streams not found when delete streams", zap.Int64s("existed-stream-ids", existedStreamIDs))
	}

	streams := make([]*rpcfb.StreamT, 0, len(prevKvs))
	for _, prevKv := range prevKvs {
		streams = append(streams, rpcfb.GetRootAsStream(prevKv.Value, 0).UnPack())
	}

	return streams, nil
}

// UpdateStreams updates the streams with the given streams and returns them.
func (e *Endpoint) UpdateStreams(ctx context.Context, streams []*rpcfb.StreamT) ([]*rpcfb.StreamT, error) {
	logger := e.lg.With(traceutil.TraceLogField(ctx))

	kvs := make([]kv.KeyValue, 0, len(streams))
	for _, stream := range streams {
		if stream.StreamId < MinStreamID {
			return nil, errors.Errorf("invalid stream id: %d < %d", stream.StreamId, MinStreamID)
		}
		streamInfo := fbutil.Marshal(stream)
		kvs = append(kvs, kv.KeyValue{
			Key:   streamPath(stream.StreamId),
			Value: streamInfo,
		})
	}

	prevKvs, err := e.BatchPut(ctx, kvs, true)
	for _, keyValue := range kvs {
		mcache.Free(keyValue.Value)
	}
	if err != nil {
		streamIDs := make([]int64, 0, len(streams))
		for _, stream := range streams {
			streamIDs = append(streamIDs, stream.StreamId)
		}
		logger.Error("failed to update stream", zap.Int64s("stream-ids", streamIDs), zap.Error(err))
		return nil, errors.Wrap(err, "update stream")
	}
	if len(prevKvs) < len(streams) {
		existedStreamIDs := streamIDsFromPaths(prevKvs)
		streamIDs := make([]int64, 0, len(streams))
		for _, stream := range streams {
			streamIDs = append(streamIDs, stream.StreamId)
		}
		logger.Warn("streams not found when update streams, will create them", zap.Int64s("existed-stream-ids", existedStreamIDs), zap.Int64s("stream-ids", streamIDs))
	}

	return streams, nil
}

// GetStream gets the stream with the given stream id.
func (e *Endpoint) GetStream(ctx context.Context, streamID int64) (*rpcfb.StreamT, error) {
	streams, err := e.GetStreams(ctx, []int64{streamID})
	if err != nil {
		return nil, err
	}
	if len(streams) == 0 {
		return nil, nil
	}

	return streams[0], nil
}

// GetStreams gets the streams with the given stream ids.
func (e *Endpoint) GetStreams(ctx context.Context, streamIDs []int64) ([]*rpcfb.StreamT, error) {
	logger := e.lg.With(traceutil.TraceLogField(ctx))

	keys := make([][]byte, len(streamIDs))
	for i, streamID := range streamIDs {
		keys[i] = streamPath(streamID)
	}

	kvs, err := e.BatchGet(ctx, keys)
	if err != nil {
		logger.Error("failed to get streams", zap.Int64s("stream-ids", streamIDs), zap.Error(err))
		return nil, errors.Wrap(err, "get streams")
	}

	streams := make([]*rpcfb.StreamT, 0, len(kvs))
	for _, streamKV := range kvs {
		if streamKV.Value == nil {
			continue
		}
		streams = append(streams, rpcfb.GetRootAsStream(streamKV.Value, 0).UnPack())
	}

	if len(streams) < len(streamIDs) {
		logger.Warn("some steams not found", zap.Int("expected-count", len(streamIDs)), zap.Int("got-count", len(streams)),
			zap.Int64s("expected-stream-ids", streamIDs), zap.Int64s("got-stream-ids", streamIDsFromPaths(kvs)))
	}

	return streams, nil
}

// ForEachStream calls the given function for every stream in the storage.
// If f returns an error, the iteration is stopped and the error is returned.
func (e *Endpoint) ForEachStream(ctx context.Context, f func(stream *rpcfb.StreamT) error) error {
	var startID = MinStreamID
	for startID >= MinStreamID {
		nextID, err := e.forEachStreamLimited(ctx, f, startID, _streamByRangeLimit)
		if err != nil {
			return err
		}
		startID = nextID
	}
	return nil
}

func (e *Endpoint) forEachStreamLimited(ctx context.Context, f func(stream *rpcfb.StreamT) error, startID int64, limit int64) (nextID int64, err error) {
	logger := e.lg.With(traceutil.TraceLogField(ctx))

	startKey := streamPath(startID)
	kvs, err := e.GetByRange(ctx, kv.Range{StartKey: startKey, EndKey: e.endStreamPath()}, limit, false)
	if err != nil {
		logger.Error("failed to get streams", zap.Int64("start-id", startID), zap.Int64("limit", limit), zap.Error(err))
		return MinStreamID - 1, errors.Wrap(err, "get streams")
	}

	for _, streamKV := range kvs {
		stream := rpcfb.GetRootAsStream(streamKV.Value, 0).UnPack()
		nextID = stream.StreamId + 1
		err = f(stream)
		if err != nil {
			return MinStreamID - 1, err
		}
	}

	if int64(len(kvs)) < limit {
		// no more streams
		nextID = MinStreamID - 1
	}
	return
}

func (e *Endpoint) endStreamPath() []byte {
	return e.GetPrefixRangeEnd([]byte(_streamPrefix))
}

func streamPath(streamID int64) []byte {
	res := make([]byte, 0, _streamKeyLen)
	res = fmt.Appendf(res, _streamFormat, streamID)
	return res
}

func streamIDsFromPaths(prevKvs []kv.KeyValue) []int64 {
	streamIDs := make([]int64, 0, len(prevKvs))
	for _, prevKv := range prevKvs {
		streamID, err := streamIDFromPath(prevKv.Key)
		if err != nil {
			continue
		}
		streamIDs = append(streamIDs, streamID)
	}
	return streamIDs
}

func streamIDFromPath(path []byte) (streamID int64, err error) {
	_, err = fmt.Sscanf(string(path), _streamFormat, &streamID)
	if err != nil {
		err = errors.Wrapf(err, "parse stream id from path %s", string(path))
	}
	return
}
