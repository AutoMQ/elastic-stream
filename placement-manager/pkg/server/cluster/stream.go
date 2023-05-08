package cluster

import (
	"context"

	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/server/id"
	"github.com/AutoMQ/placement-manager/pkg/server/storage/endpoint"
	"github.com/AutoMQ/placement-manager/pkg/server/storage/kv"
	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
)

type Stream interface {
	CreateStreams(ctx context.Context, streams []*rpcfb.StreamT) ([]*rpcfb.CreateStreamResultT, error)
	DeleteStreams(ctx context.Context, streamIDs []int64) ([]*rpcfb.StreamT, error)
	UpdateStreams(ctx context.Context, streams []*rpcfb.StreamT) ([]*rpcfb.StreamT, error)
	DescribeStreams(ctx context.Context, streamIDs []int64) ([]*rpcfb.DescribeStreamResultT, error)
}

var (
	// ErrNotLeader is returned when the current node is not the leader.
	ErrNotLeader = errors.New("not leader")
)

// CreateStreams creates streams and the first range in each stream in transaction.
// It returns ErrNotEnoughDataNodes if there are not enough data nodes to create the streams.
// It returns ErrNotLeader if the transaction failed.
func (c *RaftCluster) CreateStreams(ctx context.Context, streams []*rpcfb.StreamT) ([]*rpcfb.CreateStreamResultT, error) {
	logger := c.lg.With(zap.Int("stream-cnt", len(streams)), traceutil.TraceLogField(ctx))

	ids, err := c.sAlloc.AllocN(ctx, len(streams))
	if err != nil {
		logger.Error("failed to allocate stream ids", zap.Error(err))
		if errors.Is(err, id.ErrTxnFailed) {
			return nil, ErrNotLeader
		}
		return nil, err
	}

	params := make([]*endpoint.CreateStreamParam, 0, len(streams))
	for i, stream := range streams {
		stream.StreamId = int64(ids[i])
		nodes, err := c.chooseDataNodes(stream.ReplicaNum)
		if err != nil {
			logger.Error("failed to choose data nodes", zap.Int64("stream-id", stream.StreamId), zap.Error(err))
			return nil, err
		}
		params = append(params, &endpoint.CreateStreamParam{
			StreamT: stream,
			RangeT: &rpcfb.RangeT{
				StreamId:     stream.StreamId,
				RangeIndex:   0,
				StartOffset:  0,
				EndOffset:    _writableRangeEndOffset,
				ReplicaNodes: nodes,
			},
		})
	}

	logger.Info("start to create streams", zap.Uint64s("stream-ids", ids))
	results, err := c.storage.CreateStreams(ctx, params)
	logger.Info("finish creating streams", zap.Error(err))
	if errors.Is(err, kv.ErrTxnFailed) {
		err = ErrNotLeader
	}

	for _, result := range results {
		for _, node := range result.Range.ReplicaNodes {
			c.fillDataNodeInfo(node)
		}
	}
	return results, err
}

// DeleteStreams deletes streams in transaction.
// It returns ErrNotLeader if the transaction failed.
func (c *RaftCluster) DeleteStreams(ctx context.Context, streamIDs []int64) ([]*rpcfb.StreamT, error) {
	logger := c.lg.With(zap.Int("stream-cnt", len(streamIDs)), traceutil.TraceLogField(ctx))

	logger.Info("start to delete streams", zap.Int64s("stream-ids", streamIDs))
	streams, err := c.storage.DeleteStreams(ctx, streamIDs)
	logger.Info("finish deleting streams", zap.Error(err))
	if errors.Is(err, kv.ErrTxnFailed) {
		err = ErrNotLeader
	}

	return streams, err
}

// UpdateStreams updates streams in transaction.
// It returns ErrNotLeader if the transaction failed.
func (c *RaftCluster) UpdateStreams(ctx context.Context, streams []*rpcfb.StreamT) ([]*rpcfb.StreamT, error) {
	logger := c.lg.With(zap.Int("stream-cnt", len(streams)), traceutil.TraceLogField(ctx))

	streamIDs := make([]int64, 0, len(streams))
	for _, stream := range streams {
		streamIDs = append(streamIDs, stream.StreamId)
	}
	logger.Info("start to update streams", zap.Int64s("stream-ids", streamIDs))
	upStreams, err := c.storage.UpdateStreams(ctx, streams)
	logger.Info("finish updating streams", zap.Error(err))
	if errors.Is(err, kv.ErrTxnFailed) {
		err = ErrNotLeader
	}

	return upStreams, err
}

// DescribeStreams describes streams.
// It returns ErrNotLeader if the transaction failed.
func (c *RaftCluster) DescribeStreams(ctx context.Context, streamIDs []int64) ([]*rpcfb.DescribeStreamResultT, error) {
	logger := c.lg.With(zap.Int("stream-cnt", len(streamIDs)), traceutil.TraceLogField(ctx))

	logger.Info("start to describe streams", zap.Int64s("stream-ids", streamIDs))
	results := make([]*rpcfb.DescribeStreamResultT, 0, len(streamIDs))
	for _, streamID := range streamIDs {
		stream, err := c.storage.GetStream(ctx, streamID)
		if err != nil {
			if errors.Is(err, kv.ErrTxnFailed) {
				return nil, ErrNotLeader
			}

			results = append(results, &rpcfb.DescribeStreamResultT{
				Status: &rpcfb.StatusT{
					Code:    rpcfb.ErrorCodePM_INTERNAL_SERVER_ERROR,
					Message: err.Error(),
				},
			})
			continue
		}
		if stream == nil {
			continue
		}
		results = append(results, &rpcfb.DescribeStreamResultT{
			Stream: stream,
			Status: &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK},
		})
	}
	logger.Info("finish describing streams")
	return results, nil
}
