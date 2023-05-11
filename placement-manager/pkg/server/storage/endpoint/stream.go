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
	CreateStream(ctx context.Context, param *CreateStreamParam) (*rpcfb.StreamT, error)
	DeleteStream(ctx context.Context, streamID int64) (*rpcfb.StreamT, error)
	UpdateStream(ctx context.Context, stream *rpcfb.StreamT) (*rpcfb.StreamT, error)
	GetStream(ctx context.Context, streamID int64) (*rpcfb.StreamT, error)
	ForEachStream(ctx context.Context, f func(stream *rpcfb.StreamT) error) error
}

// CreateStream creates a new stream based on the given stream and returns it.
func (e *Endpoint) CreateStream(ctx context.Context, param *CreateStreamParam) (*rpcfb.StreamT, error) {
	logger := e.lg.With(zap.Int64("stream-id", param.StreamT.StreamId), traceutil.TraceLogField(ctx))

	kvs := make([]kv.KeyValue, 0, 5)
	kvs = append(kvs, kv.KeyValue{
		Key:   streamPath(param.StreamT.StreamId),
		Value: fbutil.Marshal(param.StreamT),
	})
	kvs = append(kvs, kv.KeyValue{
		Key:   rangePathInSteam(param.StreamT.StreamId, param.RangeT.Index),
		Value: fbutil.Marshal(param.RangeT),
	})
	for _, node := range param.RangeT.Nodes {
		kvs = append(kvs, kv.KeyValue{
			Key:   rangePathOnDataNode(node.NodeId, param.StreamT.StreamId, param.RangeT.Index),
			Value: nil,
		})
	}

	prevKVs, err := e.BatchPut(ctx, kvs, true, true)
	for _, keyValue := range kvs {
		if keyValue.Value != nil {
			mcache.Free(keyValue.Value)
		}
	}
	if err != nil {
		logger.Error("failed to save stream", zap.Error(err))
		return nil, errors.Wrap(err, "save stream")
	}
	if len(prevKVs) != 0 {
		existedStreamIDs := streamIDsFromPaths(prevKVs)
		logger.Warn("stream already exist, will override it", zap.Int64s("existed-stream-id", existedStreamIDs))
	}

	return param.StreamT, nil
}

// DeleteStream deletes the stream with the given stream id and returns it.
func (e *Endpoint) DeleteStream(ctx context.Context, streamID int64) (*rpcfb.StreamT, error) {
	logger := e.lg.With(zap.Int64("stream-id", streamID), traceutil.TraceLogField(ctx))

	prevV, err := e.Delete(ctx, streamPath(streamID), true)
	if err != nil {
		logger.Error("failed to delete stream", zap.Error(err))
		return nil, errors.Wrap(err, "delete stream")
	}
	if prevV == nil {
		logger.Warn("stream not found when delete stream")
		return nil, nil
	}

	return rpcfb.GetRootAsStream(prevV, 0).UnPack(), nil
}

// UpdateStream updates the stream with the given stream and returns it.
func (e *Endpoint) UpdateStream(ctx context.Context, stream *rpcfb.StreamT) (*rpcfb.StreamT, error) {
	logger := e.lg.With(zap.Int64("stream-id", stream.StreamId), traceutil.TraceLogField(ctx))

	if stream.StreamId < MinStreamID {
		logger.Error("invalid stream id")
		return nil, errors.Errorf("invalid stream id: %d < %d", stream.StreamId, MinStreamID)
	}

	streamInfo := fbutil.Marshal(stream)
	prevV, err := e.Put(ctx, streamPath(stream.StreamId), streamInfo, true)
	mcache.Free(streamInfo)
	if err != nil {
		logger.Error("failed to update stream", zap.Error(err))
		return nil, errors.Wrap(err, "update stream")
	}
	if prevV == nil {
		logger.Warn("stream not found when update stream, will create it")
		return nil, nil
	}

	return stream, nil
}

// GetStream gets the stream with the given stream id.
func (e *Endpoint) GetStream(ctx context.Context, streamID int64) (*rpcfb.StreamT, error) {
	logger := e.lg.With(zap.Int64("stream-id", streamID), traceutil.TraceLogField(ctx))

	v, err := e.Get(ctx, streamPath(streamID))
	if err != nil {
		logger.Error("failed to get stream", zap.Error(err))
		return nil, errors.Wrap(err, "get stream")
	}
	if v == nil {
		logger.Warn("stream not found")
		return nil, nil
	}

	return rpcfb.GetRootAsStream(v, 0).UnPack(), nil
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

func streamIDsFromPaths(prevKVs []kv.KeyValue) []int64 {
	streamIDs := make([]int64, 0, len(prevKVs))
	for _, prevKV := range prevKVs {
		streamID, err := streamIDFromPath(prevKV.Key)
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
