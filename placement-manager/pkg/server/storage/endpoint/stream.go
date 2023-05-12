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

// Stream defines operations on stream.
type Stream interface {
	CreateStream(ctx context.Context, stream *rpcfb.StreamT) (*rpcfb.StreamT, error)
	DeleteStream(ctx context.Context, streamID int64) (*rpcfb.StreamT, error)
	UpdateStream(ctx context.Context, stream *rpcfb.StreamT) (*rpcfb.StreamT, error)
	GetStream(ctx context.Context, streamID int64) (*rpcfb.StreamT, error)
	ForEachStream(ctx context.Context, f func(stream *rpcfb.StreamT) error) error
}

// CreateStream creates a new stream based on the given stream and returns it.
func (e *Endpoint) CreateStream(ctx context.Context, stream *rpcfb.StreamT) (*rpcfb.StreamT, error) {
	logger := e.lg.With(zap.Int64("stream-id", stream.StreamId), traceutil.TraceLogField(ctx))

	key := streamPath(stream.StreamId)
	value := fbutil.Marshal(stream)
	prevValue, err := e.Put(ctx, key, value, true)
	mcache.Free(value)

	if err != nil {
		logger.Error("failed to save stream", zap.Error(err))
		return nil, errors.Wrap(err, "save stream")
	}
	if prevValue != nil {
		logger.Warn("stream already exist, will override it")
	}

	return stream, nil
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

//nolint:unused
func streamIDFromPath(path []byte) (streamID int64, err error) {
	_, err = fmt.Sscanf(string(path), _streamFormat, &streamID)
	if err != nil {
		err = errors.Wrapf(err, "parse stream id from path %s", string(path))
	}
	return
}
