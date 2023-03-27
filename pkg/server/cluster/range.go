package cluster

import (
	"context"

	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/server/storage/endpoint"
	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
)

const (
	// TODO: Make it configurable.
	_sealReqTimeoutMs = 100

	_writableRangeEndOffset int64 = -1
)

var (
	// ErrRangeNotFound is returned when the specified range is not found.
	ErrRangeNotFound = errors.New("range not found")
	// ErrNoDataNodeResponded is returned when no data node responded to the seal request.
	ErrNoDataNodeResponded = errors.New("no data node responded")
)

// ListRanges lists the ranges of
// 1. a stream
// 2. a data node
// 3. a data node and a stream
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

	for _, r := range ranges {
		for _, node := range r.ReplicaNodes {
			c.fillDataNodeInfo(node.DataNode)
		}
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

// SealRange seals a range.
// It returns the current writable range and an optional error.
// It returns a nil range if and only if the ctx is done or the stream does not exist.
// It returns ErrRangeNotFound if the range does not exist.
// It returns ErrNoDataNodeResponded if no data node responded to the seal request.
// It returns ErrNotEnoughDataNodes if there are not enough data nodes to allocate.
func (c *RaftCluster) SealRange(ctx context.Context, rangeID *rpcfb.RangeIdT) (*rpcfb.RangeT, error) {
	writableRange := c.cache.WritableRange(rangeID.StreamId)
	mu := writableRange.Mu()

	select {
	case mu <- struct{}{}:
	case <-ctx.Done():
		return nil, ctx.Err()
	}
	defer func() {
		<-mu
	}()

	if writableRange.RangeT != nil && writableRange.RangeIndex > rangeID.RangeIndex {
		return writableRange.RangeT, nil
	}

	lastRange, err := c.storage.GetLastRange(ctx, rangeID.StreamId)
	if lastRange == nil || err != nil {
		// The stream does not exist.
		return nil, ErrRangeNotFound
	}
	for _, node := range lastRange.ReplicaNodes {
		c.fillDataNodeInfo(node.DataNode)
	}
	writableRange.RangeT = lastRange

	if writableRange.RangeIndex > rangeID.RangeIndex {
		// The range is already sealed.
		return writableRange.RangeT, nil
	}
	if writableRange.RangeIndex < rangeID.RangeIndex {
		// The range is not found.
		return writableRange.RangeT, ErrRangeNotFound
	}

	// Here, writableRange.RangeIndex == rangeID.RangeIndex.
	endOffset, err := c.sealRangeOnDataNode(ctx, writableRange.RangeT)
	if err != nil {
		return writableRange.RangeT, err
	}

	oldRange, newRange, err := c.newRange(ctx, writableRange.RangeT, endOffset)
	if err != nil {
		return writableRange.RangeT, err
	}

	newRange, err = c.storage.SealRange(ctx, oldRange, newRange)
	if err != nil {
		return writableRange.RangeT, err
	}
	for _, node := range newRange.ReplicaNodes {
		c.fillDataNodeInfo(node.DataNode)
	}
	writableRange.RangeT = newRange

	return writableRange.RangeT, nil
}

func (c *RaftCluster) sealRangeOnDataNode(ctx context.Context, writableRange *rpcfb.RangeT) (endOffset int64, err error) {
	logger := c.lg.With(zap.Int64("range-stream-id", writableRange.StreamId), zap.Int32("range-index", writableRange.RangeIndex), traceutil.TraceLogField(ctx))

	req := &protocol.SealRangesRequest{SealRangesRequestT: rpcfb.SealRangesRequestT{
		Ranges:    []*rpcfb.RangeIdT{{StreamId: writableRange.StreamId, RangeIndex: writableRange.RangeIndex}},
		TimeoutMs: _sealReqTimeoutMs,
	}}
	ch := make(chan *rpcfb.RangeT)
	for _, node := range writableRange.ReplicaNodes {
		go func(node *rpcfb.ReplicaNodeT) {
			resp, err := c.client.SealRanges(req, node.DataNode.AdvertiseAddr)
			if resp == nil || err != nil {
				logger.Error("failed to seal range on data node: request failed", zap.Int32("data-node-id", node.DataNode.NodeId), zap.Error(err))
				ch <- nil
				return
			}
			if resp.Status.Code != rpcfb.ErrorCodeOK {
				logger.Error("failed to seal range on data node: error response", zap.Int32("data-node-id", node.DataNode.NodeId),
					zap.String("status-code", resp.Status.Code.String()), zap.String("status-msg", resp.Status.Message))
				ch <- nil
				return
			}
			for _, result := range resp.SealResponses {
				if result.Range.StreamId == writableRange.StreamId && result.Range.RangeIndex == writableRange.RangeIndex {
					if result.Status.Code != rpcfb.ErrorCodeOK {
						logger.Error("failed to seal range on data node: error status", zap.Int32("data-node-id", node.DataNode.NodeId),
							zap.String("status-code", result.Status.Code.String()), zap.String("status-msg", result.Status.Message))
						ch <- nil
						return
					}
					ch <- result.Range
					return
				}
			}
			logger.Error("failed to seal range on data node: no response for the range", zap.Int32("data-node-id", node.DataNode.NodeId))
			ch <- nil
		}(node)
	}

	minEndOffset := _writableRangeEndOffset
	for range writableRange.ReplicaNodes {
		var r *rpcfb.RangeT
		select {
		case <-ctx.Done():
			return _writableRangeEndOffset, ctx.Err()
		case r = <-ch:
		}
		if r == nil {
			continue
		}

		if minEndOffset == _writableRangeEndOffset {
			minEndOffset = r.EndOffset
			continue
		}
		if r.EndOffset < minEndOffset {
			minEndOffset = r.EndOffset
		}
	}
	if minEndOffset == _writableRangeEndOffset {
		return _writableRangeEndOffset, ErrNoDataNodeResponded
	}

	return minEndOffset, nil
}

func (c *RaftCluster) newRange(ctx context.Context, r *rpcfb.RangeT, endOffset int64) (old *rpcfb.RangeT, new *rpcfb.RangeT, err error) {
	logger := c.lg.With(zap.Int64("range-stream-id", r.StreamId), zap.Int32("range-index", r.RangeIndex), traceutil.TraceLogField(ctx))

	oldNodes := make([]*rpcfb.ReplicaNodeT, 0, len(r.ReplicaNodes))
	for _, node := range r.ReplicaNodes {
		oldNodes = append(oldNodes, &rpcfb.ReplicaNodeT{
			DataNode:  node.DataNode,
			IsPrimary: node.IsPrimary,
		})
	}
	old = &rpcfb.RangeT{
		StreamId:     r.StreamId,
		RangeIndex:   r.RangeIndex,
		StartOffset:  r.StartOffset,
		EndOffset:    endOffset,
		ReplicaNodes: oldNodes,
	}

	newNodes, err := c.chooseDataNodes(int8(len(r.ReplicaNodes)))
	if err != nil {
		logger.Error("failed to choose data nodes", zap.Int64("stream-id", r.StreamId), zap.Error(err))
		return nil, nil, err
	}
	new = &rpcfb.RangeT{
		StreamId:     r.StreamId,
		RangeIndex:   r.RangeIndex + 1,
		StartOffset:  endOffset,
		EndOffset:    _writableRangeEndOffset,
		ReplicaNodes: newNodes,
	}
	return
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
