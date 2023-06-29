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
	_dataNodeIDFormat = _int32Format
	_dataNodeIDLen    = _int32Len

	_dataNodePath   = "data-nodes"
	_dataNodePrefix = _dataNodePath + kv.KeySeparator
	_dataNodeFormat = _dataNodePath + kv.KeySeparator + _dataNodeIDFormat
	_dataNodeKeyLen = len(_dataNodePath) + len(kv.KeySeparator) + _dataNodeIDLen

	_dataNodeByRangeLimit = 1e4
)

type DataNode interface {
	SaveDataNode(ctx context.Context, dataNode *rpcfb.DataNodeT) (*rpcfb.DataNodeT, error)
	ForEachDataNode(ctx context.Context, f func(dataNode *rpcfb.DataNodeT) error) error
}

// SaveDataNode creates or updates the given data node and returns it.
func (e *Endpoint) SaveDataNode(ctx context.Context, dataNode *rpcfb.DataNodeT) (*rpcfb.DataNodeT, error) {
	logger := e.lg.With(zap.Int32("node-id", dataNode.NodeId), traceutil.TraceLogField(ctx))

	if dataNode.NodeId < MinDataNodeID {
		logger.Error("invalid data node id")
		return nil, errors.Errorf("invalid data node id: %d < %d", dataNode.NodeId, MinDataNodeID)
	}

	key := dataNodePath(dataNode.NodeId)
	value := fbutil.Marshal(dataNode)
	defer mcache.Free(value)

	_, err := e.Put(ctx, key, value, false)
	if err != nil {
		logger.Error("failed to save data node", zap.Error(err))
		return nil, errors.Wrap(err, "save data node")
	}

	return dataNode, nil
}

// ForEachDataNode calls the given function for every data node in the storage.
// If f returns an error, the iteration is stopped and the error is returned.
func (e *Endpoint) ForEachDataNode(ctx context.Context, f func(dataNode *rpcfb.DataNodeT) error) error {
	var startID = MinDataNodeID
	for startID >= MinDataNodeID {
		nextID, err := e.forEachDataNodeLimited(ctx, f, startID, _dataNodeByRangeLimit)
		if err != nil {
			return err
		}
		startID = nextID
	}
	return nil
}

func (e *Endpoint) forEachDataNodeLimited(ctx context.Context, f func(dataNode *rpcfb.DataNodeT) error, startID int32, limit int64) (nextID int32, err error) {
	logger := e.lg.With(traceutil.TraceLogField(ctx))

	startKey := dataNodePath(startID)
	kvs, err := e.GetByRange(ctx, kv.Range{StartKey: startKey, EndKey: e.endDataNodePath()}, limit, false)
	if err != nil {
		logger.Error("failed to get data nodes", zap.Int32("start-id", startID), zap.Int64("limit", limit), zap.Error(err))
		return MinDataNodeID - 1, errors.Wrap(err, "get data nodes")
	}

	for _, keyValue := range kvs {
		dataNode := rpcfb.GetRootAsDataNode(keyValue.Value, 0).UnPack()
		nextID = dataNode.NodeId + 1
		err = f(dataNode)
		if err != nil {
			return MinDataNodeID - 1, err
		}
	}

	if int64(len(kvs)) < limit {
		// no more data nodes
		nextID = MinDataNodeID - 1
	}
	return
}

func (e *Endpoint) endDataNodePath() []byte {
	return e.GetPrefixRangeEnd([]byte(_dataNodePrefix))
}

func dataNodePath(nodeID int32) []byte {
	res := make([]byte, 0, _dataNodeKeyLen)
	res = fmt.Appendf(res, _dataNodeFormat, nodeID)
	return res
}
