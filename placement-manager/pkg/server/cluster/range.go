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

	// ErrInvalidRangeIndex is returned when the range index is invalid.
	ErrInvalidRangeIndex = errors.New("invalid range index")
	// ErrCreateBeforeSeal is returned when the last range is not sealed.
	ErrCreateBeforeSeal = errors.New("create range before sealing the previous one")
	// ErrInvalidStartOffset is returned when the end offset is invalid.
	ErrInvalidStartOffset = errors.New("invalid start offset")
)

type Range interface {
	ListRange(ctx context.Context, criteria *rpcfb.ListRangeCriteriaT) (ranges []*rpcfb.RangeT, err error)
	SealRange(ctx context.Context, r *rpcfb.RangeT) (*rpcfb.RangeT, error)
	CreateRange(ctx context.Context, r *rpcfb.RangeT) (*rpcfb.RangeT, error)
}

// ListRange lists ranges of
// 1. a stream
// 2. a data node
// 3. a data node and a stream
// It returns ErrNotLeader if the current PM node is not the leader.
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

// SealRange seals a range. It returns the sealed range if success.
// It returns ErrNotLeader if the current PM node is not the leader.
// It returns ErrStreamNotFound if the stream does not exist.
// It returns ErrRangeNotFound if the range does not exist.
// It returns ErrRangeAlreadySealed if the range is already sealed.
// It returns ErrInvalidEndOffset if the end offset is invalid.
func (c *RaftCluster) SealRange(ctx context.Context, r *rpcfb.RangeT) (*rpcfb.RangeT, error) {
	lastRange, err := c.getLastRange(ctx, r.StreamId)
	if err != nil {
		return nil, err
	}

	if r.Index > lastRange.Index {
		// Range not found.
		return nil, errors.Wrapf(ErrRangeNotFound, "range %d not found in stream %d", r.Index, r.StreamId)
	}
	if r.Index < lastRange.Index || !isWritable(lastRange) {
		// Range already sealed.
		return nil, ErrRangeAlreadySealed
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

	sealedRange, err := c.sealRangeLocked(ctx, lastRange, r.End)
	if err != nil {
		return nil, err
	}
	return sealedRange, nil
}

// CreateRange creates a range. It returns the created range if success.
// It returns ErrNotLeader if the current PM node is not the leader.
// It returns ErrStreamNotFound if the stream does not exist.
// It returns ErrInvalidRangeIndex if the range index is invalid.
// It returns ErrCreateBeforeSeal if the last range is not sealed.
// It returns ErrInvalidStartOffset if the start offset is invalid.
// It returns ErrNotEnoughDataNodes if there are not enough data nodes to allocate.
func (c *RaftCluster) CreateRange(ctx context.Context, r *rpcfb.RangeT) (*rpcfb.RangeT, error) {
	lastRange, err := c.getLastRange(ctx, r.StreamId)
	if err != nil {
		return nil, err
	}

	if lastRange == nil {
		// The stream exists, but no range in it.
		if r.Index != 0 {
			return nil, errors.Wrapf(ErrInvalidRangeIndex, "invalid range index %d (should be 0) in stream %d", r.Index, r.StreamId)
		}
	}
	if isWritable(lastRange) {
		// The last range is writable.
		return nil, errors.Wrapf(ErrCreateBeforeSeal, "create range before sealing the last range %d in stream %d", lastRange.Index, r.StreamId)
	}
	if r.Index != lastRange.Index+1 {
		// The range index is not continuous.
		return nil, errors.Wrapf(ErrInvalidRangeIndex, "invalid range index %d (should be %d) in stream %d", r.Index, lastRange.Index+1, r.StreamId)
	}
	if r.Start != lastRange.End {
		// The range start is not continuous.
		return nil, errors.Wrapf(ErrInvalidStartOffset, "invalid range start %d (should be %d) for range %d in stream %d", r.Start, lastRange.End, r.Index, r.StreamId)
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

	newRange, err := c.newRangeLocked(ctx, lastRange)
	if err != nil {
		return nil, err
	}
	c.fillDataNodesInfo(newRange.Nodes)
	return newRange, nil
}

func (c *RaftCluster) getLastRange(ctx context.Context, streamID int64) (*rpcfb.RangeT, error) {
	r, err := c.storage.GetLastRange(ctx, streamID)
	if err != nil {
		if errors.Is(err, kv.ErrTxnFailed) {
			err = ErrNotLeader
		}
		return nil, err
	}
	if r == nil {
		stream, err := c.storage.GetStream(ctx, streamID)
		if err != nil {
			if errors.Is(err, kv.ErrTxnFailed) {
				err = ErrNotLeader
			}
			return nil, err
		}
		if stream == nil {
			return nil, errors.Wrapf(ErrStreamNotFound, "stream %d not found", streamID)
		}
		return nil, nil
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
// It should be called with the stream lock held.
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
// It should be called with the stream lock held.
func (c *RaftCluster) newRangeLocked(ctx context.Context, newRange *rpcfb.RangeT) (*rpcfb.RangeT, error) {
	logger := c.lg.With(zap.Int64("stream-id", newRange.StreamId), zap.Int32("range-index", newRange.Index), traceutil.TraceLogField(ctx))

	nodes, err := c.chooseDataNodes(int8(len(newRange.Nodes)))
	if err != nil {
		logger.Error("failed to choose data nodes", zap.Error(err))
		return nil, err
	}

	nr := &rpcfb.RangeT{
		StreamId: newRange.StreamId,
		Index:    newRange.Index,
		Start:    newRange.Start,
		End:      _writableRangeEnd,
		Nodes:    nodes,
	}

	err = c.storage.CreateRange(ctx, nr)
	if err != nil {
		return nil, err
	}

	logger.Info("range created", zap.Int64("start", nr.Start))
	return nr, nil
}

func isWritable(r *rpcfb.RangeT) bool {
	return r.End == _writableRangeEnd
}
