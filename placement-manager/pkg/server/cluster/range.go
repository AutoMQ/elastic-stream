package cluster

import (
	"context"

	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/server/storage/endpoint"
	"github.com/AutoMQ/placement-manager/pkg/server/storage/kv"
	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
)

const (
	_writableRangeEndOffset int64 = -1
)

var (
	// ErrRangeNotFound is returned when the specified range is not found.
	ErrRangeNotFound = errors.New("range not found")
	// ErrRangeAlreadySealed is returned when the specified range is already sealed.
	ErrRangeAlreadySealed = errors.New("range already sealed")
	// ErrInvalidEndOffset is returned when the end offset is invalid.
	ErrInvalidEndOffset = errors.New("invalid end offset")
)

type Range interface {
	ListRanges(ctx context.Context, rangeCriteria *rpcfb.RangeCriteriaT) (ranges []*rpcfb.RangeT, err error)
	SealRange(ctx context.Context, entry *rpcfb.SealRangeEntryT) (*rpcfb.RangeT, error)
}

// ListRanges lists the ranges of
// 1. a stream
// 2. a data node
// 3. a data node and a stream
// It returns ErrNotLeader if the transaction failed.
func (c *RaftCluster) ListRanges(ctx context.Context, rangeCriteria *rpcfb.RangeCriteriaT) (ranges []*rpcfb.RangeT, err error) {
	byStream := rangeCriteria.StreamId >= endpoint.MinStreamID
	byDataNode := rangeCriteria.NodeId >= endpoint.MinDataNodeID
	switch {
	case byStream && byDataNode:
		ranges, err = c.listRangesOnDataNodeInStream(ctx, rangeCriteria.StreamId, rangeCriteria.NodeId)
	case byStream && !byDataNode:
		ranges, err = c.listRangesInStream(ctx, rangeCriteria.StreamId)
	case !byStream && byDataNode:
		ranges, err = c.listRangesOnDataNode(ctx, rangeCriteria.NodeId)
	default:
	}
	if errors.Is(err, kv.ErrTxnFailed) {
		err = ErrNotLeader
	}

	for _, r := range ranges {
		c.fillDataNodesInfo(r.ReplicaNodes)
	}
	return
}

// listRangesOnDataNodeInStream lists the ranges on a data node in a stream.
func (c *RaftCluster) listRangesOnDataNodeInStream(ctx context.Context, streamID int64, dataNodeID int32) ([]*rpcfb.RangeT, error) {
	logger := c.lg.With(zap.Int64("stream-id", streamID), zap.Int32("data-node-id", dataNodeID), traceutil.TraceLogField(ctx))

	logger.Info("start to list ranges on data node in stream")
	rangeIDs, err := c.storage.GetRangeIDsByDataNodeAndStream(ctx, streamID, dataNodeID)
	if err != nil {
		return nil, err
	}

	ranges, err := c.storage.GetRanges(ctx, rangeIDs)
	if err != nil {
		return nil, err
	}

	logger.Info("finish listing ranges on data node in stream", zap.Int("range-cnt", len(ranges)))
	return ranges, nil
}

// listRangesInStream lists the ranges of a stream.
func (c *RaftCluster) listRangesInStream(ctx context.Context, streamID int64) ([]*rpcfb.RangeT, error) {
	logger := c.lg.With(zap.Int64("stream-id", streamID), traceutil.TraceLogField(ctx))

	logger.Info("start to list ranges in stream")
	ranges, err := c.storage.GetRangesByStream(ctx, streamID)
	logger.Info("finish listing ranges in stream", zap.Int("range-cnt", len(ranges)), zap.Error(err))

	return ranges, err
}

// listRangesOnDataNode lists the ranges on a data node.
func (c *RaftCluster) listRangesOnDataNode(ctx context.Context, dataNodeID int32) ([]*rpcfb.RangeT, error) {
	logger := c.lg.With(zap.Int32("data-node-id", dataNodeID), traceutil.TraceLogField(ctx))

	logger.Info("start to list ranges on data node")
	rangeIDs, err := c.storage.GetRangeIDsByDataNode(ctx, dataNodeID)
	if err != nil {
		return nil, err
	}

	ranges, err := c.storage.GetRanges(ctx, rangeIDs)
	if err != nil {
		return nil, err
	}

	logger.Info("finish listing ranges on data node", zap.Int("range-cnt", len(ranges)))
	return ranges, nil
}

// SealRange seals a range.
// It returns the current writable range if entry.Renew == true.
// It returns ErrRangeNotFound if the range does not exist.
// It returns ErrRangeAlreadySealed if the range is already sealed.
// It returns ErrInvalidEndOffset if the end offset is invalid.
// It returns ErrNotEnoughDataNodes if there are not enough data nodes to allocate.
func (c *RaftCluster) SealRange(ctx context.Context, entry *rpcfb.SealRangeEntryT) (*rpcfb.RangeT, error) {
	lastRange, err := c.getLastRange(ctx, entry.Range.StreamId)
	if err != nil {
		return nil, err
	}
	mu := c.sealMu(entry.Range.StreamId)

	select {
	case mu <- struct{}{}:
	case <-ctx.Done():
		return nil, ctx.Err()
	}
	defer func() {
		<-mu
	}()

	var rerr error
	switch {
	case isWritable(lastRange) && entry.Range.RangeIndex == lastRange.RangeIndex:
		sealedRange, err := c.sealRangeLocked(ctx, lastRange, entry.End)
		if err != nil {
			return nil, err
		}
		lastRange = sealedRange
	case entry.Range.RangeIndex > lastRange.RangeIndex:
		// Range not found.
		rerr = errors.Wrapf(ErrRangeNotFound, "range %d not found in stream %d", entry.Range.RangeIndex, entry.Range.StreamId)
	default:
		// The range is already sealed.
		rerr = ErrRangeAlreadySealed
	}

	var writableRange *rpcfb.RangeT
	if entry.Renew {
		if !isWritable(lastRange) {
			newRange, err := c.newRangeLocked(ctx, lastRange)
			if err != nil {
				return nil, err
			}
			lastRange = newRange
		}
		writableRange = lastRange
		c.fillDataNodesInfo(writableRange.ReplicaNodes)
	}

	return writableRange, rerr
}

func (c *RaftCluster) getLastRange(ctx context.Context, streamID int64) (*rpcfb.RangeT, error) {
	r, err := c.storage.GetLastRange(ctx, streamID)
	if err != nil {
		if errors.Is(err, kv.ErrTxnFailed) {
			err = ErrNotLeader
		} else {
			err = errors.Wrapf(ErrRangeNotFound, "stream %d does not exist", streamID)
		}
		return nil, err
	}
	return r, nil
}

func (c *RaftCluster) sealMu(streamID int64) chan struct{} {
	if sealMu, ok := c.sealMus[streamID]; ok {
		return sealMu
	}
	c.sealMus[streamID] = make(chan struct{}, 1)
	return c.sealMus[streamID]
}

// sealRangeLocked seals a range and saves it to the storage.
// It returns the sealed range if the range is sealed successfully.
// It should be called with the Range lock held.
func (c *RaftCluster) sealRangeLocked(ctx context.Context, lastRange *rpcfb.RangeT, endOffset int64) (*rpcfb.RangeT, error) {
	logger := c.lg.With(zap.Int64("stream-id", lastRange.StreamId), zap.Int32("range-index", lastRange.RangeIndex), traceutil.TraceLogField(ctx))

	if endOffset < lastRange.StartOffset {
		logger.Error("invalid end offset", zap.Int64("end-offset", endOffset), zap.Int64("start-offset", lastRange.StartOffset))
		return nil, errors.Wrapf(ErrInvalidEndOffset, "invalid end offset %d (< start offset %d) for range %d in stream %d",
			endOffset, lastRange.StartOffset, lastRange.RangeIndex, lastRange.StreamId)
	}

	sealedRange := &rpcfb.RangeT{
		StreamId:     lastRange.StreamId,
		RangeIndex:   lastRange.RangeIndex,
		StartOffset:  lastRange.StartOffset,
		EndOffset:    endOffset,
		ReplicaNodes: eraseDataNodesInfo(lastRange.ReplicaNodes),
	}

	_, err := c.storage.UpdateRange(ctx, sealedRange)
	if err != nil {
		return nil, err
	}

	logger.Info("range sealed", zap.Int64("end-offset", endOffset))
	return sealedRange, nil
}

// newRangeLocked creates a new range and saves it to the storage.
// It returns the new range if the range is created successfully.
// It should be called with the Range lock held.
func (c *RaftCluster) newRangeLocked(ctx context.Context, lastRange *rpcfb.RangeT) (*rpcfb.RangeT, error) {
	logger := c.lg.With(zap.Int64("stream-id", lastRange.StreamId), zap.Int32("sealed-range-index", lastRange.RangeIndex), traceutil.TraceLogField(ctx))

	nodes, err := c.chooseDataNodes(int8(len(lastRange.ReplicaNodes)))
	if err != nil {
		logger.Error("failed to choose data nodes", zap.Error(err))
		return nil, err
	}

	newRange := &rpcfb.RangeT{
		StreamId:     lastRange.StreamId,
		RangeIndex:   lastRange.RangeIndex + 1,
		StartOffset:  lastRange.EndOffset,
		EndOffset:    _writableRangeEndOffset,
		ReplicaNodes: nodes,
	}

	err = c.storage.CreateRange(ctx, newRange)
	if err != nil {
		return nil, err
	}

	logger.Info("range created", zap.Int32("range-index", newRange.RangeIndex), zap.Int64("start-offset", newRange.StartOffset))
	return newRange, nil
}

func isWritable(r *rpcfb.RangeT) bool {
	return r.EndOffset == _writableRangeEndOffset
}
