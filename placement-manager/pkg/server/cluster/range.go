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
	_writableRangeEnd int64 = -1
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
	ListRange(ctx context.Context, criteria *rpcfb.ListRangeCriteriaT) (ranges []*rpcfb.RangeT, err error)
	SealRange(ctx context.Context, r *rpcfb.RangeT, renew bool) (*rpcfb.RangeT, error)
}

// ListRange lists ranges of
// 1. a stream
// 2. a data node
// 3. a data node and a stream
// It returns ErrNotLeader if the transaction failed.
func (c *RaftCluster) ListRange(ctx context.Context, criteria *rpcfb.ListRangeCriteriaT) (ranges []*rpcfb.RangeT, err error) {
	byStream := criteria.StreamId >= endpoint.MinStreamID
	byDataNode := criteria.NodeId >= endpoint.MinDataNodeID
	switch {
	case byStream && byDataNode:
		ranges, err = c.listRangeOnDataNodeInStream(ctx, criteria.StreamId, criteria.NodeId)
	case byStream && !byDataNode:
		ranges, err = c.listRangeInStream(ctx, criteria.StreamId)
	case !byStream && byDataNode:
		ranges, err = c.listRangeOnDataNode(ctx, criteria.NodeId)
	default:
	}
	if errors.Is(err, kv.ErrTxnFailed) {
		err = ErrNotLeader
	}

	for _, r := range ranges {
		c.fillDataNodesInfo(r.Nodes)
	}
	return
}

// listRangeOnDataNodeInStream lists ranges on a data node in a stream.
func (c *RaftCluster) listRangeOnDataNodeInStream(ctx context.Context, streamID int64, dataNodeID int32) ([]*rpcfb.RangeT, error) {
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

// listRangeInStream lists ranges in a stream.
func (c *RaftCluster) listRangeInStream(ctx context.Context, streamID int64) ([]*rpcfb.RangeT, error) {
	logger := c.lg.With(zap.Int64("stream-id", streamID), traceutil.TraceLogField(ctx))

	logger.Info("start to list ranges in stream")
	ranges, err := c.storage.GetRangesByStream(ctx, streamID)
	logger.Info("finish listing ranges in stream", zap.Int("range-cnt", len(ranges)), zap.Error(err))

	return ranges, err
}

// listRangeOnDataNode lists ranges on a data node.
func (c *RaftCluster) listRangeOnDataNode(ctx context.Context, dataNodeID int32) ([]*rpcfb.RangeT, error) {
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
func (c *RaftCluster) SealRange(ctx context.Context, r *rpcfb.RangeT, renew bool) (*rpcfb.RangeT, error) {
	lastRange, err := c.getLastRange(ctx, r.StreamId)
	if err != nil {
		return nil, err
	}
	mu := c.sealMu(r.StreamId)

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
	case isWritable(lastRange) && r.Index == lastRange.Index:
		sealedRange, err := c.sealRangeLocked(ctx, lastRange, r.End)
		if err != nil {
			return nil, err
		}
		lastRange = sealedRange
	case r.Index > lastRange.Index:
		// Range not found.
		rerr = errors.Wrapf(ErrRangeNotFound, "range %d not found in stream %d", r.Index, r.StreamId)
	default:
		// The range is already sealed.
		rerr = ErrRangeAlreadySealed
	}

	var writableRange *rpcfb.RangeT
	if renew {
		if !isWritable(lastRange) {
			newRange, err := c.newRangeLocked(ctx, lastRange)
			if err != nil {
				return nil, err
			}
			lastRange = newRange
		}
		writableRange = lastRange
		c.fillDataNodesInfo(writableRange.Nodes)
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
func (c *RaftCluster) sealRangeLocked(ctx context.Context, lastRange *rpcfb.RangeT, end int64) (*rpcfb.RangeT, error) {
	logger := c.lg.With(zap.Int64("stream-id", lastRange.StreamId), zap.Int32("range-index", lastRange.Index), traceutil.TraceLogField(ctx))

	if end < lastRange.Start {
		logger.Error("invalid end offset", zap.Int64("end", end), zap.Int64("start", lastRange.Start))
		return nil, errors.Wrapf(ErrInvalidEndOffset, "invalid end offset %d (< start offset %d) for range %d in stream %d",
			end, lastRange.Start, lastRange.Index, lastRange.StreamId)
	}

	sealedRange := &rpcfb.RangeT{
		StreamId: lastRange.StreamId,
		Index:    lastRange.Index,
		Start:    lastRange.Start,
		End:      end,
		Nodes:    eraseDataNodesInfo(lastRange.Nodes),
	}

	_, err := c.storage.UpdateRange(ctx, sealedRange)
	if err != nil {
		return nil, err
	}

	logger.Info("range sealed", zap.Int64("end", end))
	return sealedRange, nil
}

// newRangeLocked creates a new range and saves it to the storage.
// It returns the new range if the range is created successfully.
// It should be called with the Range lock held.
func (c *RaftCluster) newRangeLocked(ctx context.Context, lastRange *rpcfb.RangeT) (*rpcfb.RangeT, error) {
	logger := c.lg.With(zap.Int64("stream-id", lastRange.StreamId), zap.Int32("sealed-range-index", lastRange.Index), traceutil.TraceLogField(ctx))

	nodes, err := c.chooseDataNodes(int8(len(lastRange.Nodes)))
	if err != nil {
		logger.Error("failed to choose data nodes", zap.Error(err))
		return nil, err
	}

	newRange := &rpcfb.RangeT{
		StreamId: lastRange.StreamId,
		Index:    lastRange.Index + 1,
		Start:    lastRange.End,
		End:      _writableRangeEnd,
		Nodes:    nodes,
	}

	err = c.storage.CreateRange(ctx, newRange)
	if err != nil {
		return nil, err
	}

	logger.Info("range created", zap.Int32("range-index", newRange.Index), zap.Int64("start", newRange.Start))
	return newRange, nil
}

func isWritable(r *rpcfb.RangeT) bool {
	return r.End == _writableRangeEnd
}
