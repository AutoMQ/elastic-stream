package cluster

import (
	"context"

	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster/cache"
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
	byDataNode := rangeCriteria.DataNode != nil && rangeCriteria.DataNode.NodeId >= endpoint.MinDataNodeID
	switch {
	case byStream && byDataNode:
		ranges, err = c.listRangesOnDataNodeInStream(ctx, rangeCriteria.StreamId, rangeCriteria.DataNode.NodeId)
	case byStream && !byDataNode:
		ranges, err = c.listRangesInStream(ctx, rangeCriteria.StreamId)
	case !byStream && byDataNode:
		ranges, err = c.listRangesOnDataNode(ctx, rangeCriteria.DataNode.NodeId)
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

	ranges, err := c.getRanges(ctx, rangeIDs, logger)
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

	ranges, err := c.getRanges(ctx, rangeIDs, logger)
	if err != nil {
		return nil, err
	}

	logger.Info("finish listing ranges on data node", zap.Int("range-cnt", len(ranges)))
	return ranges, nil
}

func (c *RaftCluster) getRanges(ctx context.Context, rangeIDs []*rpcfb.RangeIdT, logger *zap.Logger) ([]*rpcfb.RangeT, error) {
	ranges := make([]*rpcfb.RangeT, 0, len(rangeIDs))
	for _, rangeID := range rangeIDs {
		r, err := c.storage.GetRange(ctx, rangeID)
		if err != nil {
			return nil, err
		}
		if r == nil {
			logger.Warn("range not found", zap.Int64("range-stream-id", rangeID.StreamId), zap.Int32("range-index", rangeID.RangeIndex))
			continue
		}
		ranges = append(ranges, r)
	}
	return ranges, nil
}

// SealRange seals a range.
// It returns the current writable range if entry.Renew == true.
// It returns ErrRangeNotFound if the range does not exist.
// It returns ErrNotEnoughDataNodes if there are not enough data nodes to allocate.
// It returns ErrRangeAlreadySealed if the range is already sealed.
func (c *RaftCluster) SealRange(ctx context.Context, entry *rpcfb.SealRangeEntryT) (*rpcfb.RangeT, error) {
	lastRange, err := c.getLastRange(ctx, entry.Range.StreamId)
	if err != nil {
		return nil, err
	}
	mu := lastRange.Mu()

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
	case isWritable(lastRange.RangeT) && entry.Range.RangeIndex == lastRange.RangeIndex:
		err := c.sealRangeLocked(ctx, lastRange, entry.End)
		if err != nil {
			return nil, err
		}
	case entry.Range.RangeIndex > lastRange.RangeIndex:
		// Range not found.
		rerr = errors.Wrapf(ErrRangeNotFound, "range %d not found in stream %d", entry.Range.RangeIndex, entry.Range.StreamId)
	default:
		// The range is already sealed.
		rerr = ErrRangeAlreadySealed
	}

	var writableRange *rpcfb.RangeT
	if entry.Renew {
		if !isWritable(lastRange.RangeT) {
			err := c.newRangeLocked(ctx, lastRange)
			if err != nil {
				return nil, err
			}
		}
		writableRange = lastRange.RangeT
		c.fillDataNodesInfo(writableRange.ReplicaNodes)
	}

	return writableRange, rerr
}

// sealRangeLocked seals a range and saves it to the storage.
// The sealed range will be set to the cache if the operation succeeds.
// It should be called with the Range lock held.
func (c *RaftCluster) sealRangeLocked(ctx context.Context, lastRange *cache.Range, endOffset int64) error {
	logger := c.lg.With(zap.Int64("stream-id", lastRange.StreamId), zap.Int32("range-index", lastRange.RangeIndex), traceutil.TraceLogField(ctx))

	if endOffset < lastRange.StartOffset {
		logger.Error("invalid end offset", zap.Int64("end-offset", endOffset), zap.Int64("start-offset", lastRange.StartOffset))
		return errors.Errorf("invalid end offset %d (< start offset %d) for range %d in stream %d",
			endOffset, lastRange.StartOffset, lastRange.RangeIndex, lastRange.StreamId)
	}

	sealedRange := &rpcfb.RangeT{
		StreamId:     lastRange.StreamId,
		RangeIndex:   lastRange.RangeIndex,
		StartOffset:  lastRange.StartOffset,
		EndOffset:    endOffset,
		ReplicaNodes: eraseDataNodesInfo(lastRange.ReplicaNodes),
	}

	prevRange, err := c.storage.UpdateRange(ctx, sealedRange)
	if err != nil {
		return err
	}
	// check the range in the storage is the same as the last range in the cache
	if prevRange != nil && (prevRange.StartOffset != lastRange.StartOffset || prevRange.EndOffset != lastRange.EndOffset) {
		// if everything is ok, we should never see this error
		logger.Error("range in storage is different from the last range in cache", zap.Any("range-in-storage", prevRange), zap.Any("range-in-cache", lastRange.RangeT))
		// reset cache
		lastRange.RangeT = nil
	}

	lastRange.RangeT = sealedRange
	logger.Info("range sealed", zap.Int64("end-offset", endOffset))
	return nil
}

// newRangeLocked creates a new range and saves it to the storage.
// The new range will be set to the cache if the operation succeeds.
// It should be called with the Range lock held.
func (c *RaftCluster) newRangeLocked(ctx context.Context, lastRange *cache.Range) error {
	logger := c.lg.With(zap.Int64("stream-id", lastRange.StreamId), zap.Int32("sealed-range-index", lastRange.RangeIndex), traceutil.TraceLogField(ctx))

	nodes, err := c.chooseDataNodes(int8(len(lastRange.ReplicaNodes)))
	if err != nil {
		logger.Error("failed to choose data nodes", zap.Error(err))
		return err
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
		return err
	}

	lastRange.RangeT = newRange
	logger.Info("range created", zap.Int32("range-index", newRange.RangeIndex), zap.Int64("start-offset", newRange.StartOffset))
	return nil
}

func (c *RaftCluster) getLastRange(ctx context.Context, streamID int64) (lastRange *cache.Range, err error) {
	lastRange = c.cache.LastRange(streamID)

	mu := lastRange.Mu()
	select {
	case mu <- struct{}{}:
	case <-ctx.Done():
		return nil, ctx.Err()
	}
	defer func() {
		<-mu
	}()

	if lastRange.RangeT != nil {
		return
	}

	r, err := c.storage.GetLastRange(ctx, streamID)
	if err != nil {
		if errors.Is(err, kv.ErrTxnFailed) {
			err = ErrNotLeader
		} else {
			err = errors.Wrapf(ErrRangeNotFound, "stream %d does not exist", streamID)
		}
		return
	}

	lastRange.RangeT = r
	return
}

func isWritable(r *rpcfb.RangeT) bool {
	return r.EndOffset == _writableRangeEndOffset
}
