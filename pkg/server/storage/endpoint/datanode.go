package endpoint

import (
	"fmt"

	"github.com/bytedance/gopkg/lang/mcache"
	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/server/storage/kv"
	"github.com/AutoMQ/placement-manager/pkg/util/fbutil"
)

const (
	_dataNodePath       = "data-node"
	_dataNodePrefix     = _dataNodePath + kv.KeySeparator
	_dataNodeFormat     = _dataNodePath + kv.KeySeparator + "%011d" // max length of int32 is 11
	_dataNodeKeyLen     = len(_dataNodePath) + len(kv.KeySeparator) + 11
	_dataNodeRangeLimit = 1e4
)

type DataNode interface {
	SaveDataNode(dataNode *rpcfb.DataNodeT) (*rpcfb.DataNodeT, error)
	ForEachDataNode(f func(dataNode *rpcfb.DataNodeT) error) error
}

// SaveDataNode creates or updates the given data node and returns it.
func (e *Endpoint) SaveDataNode(dataNode *rpcfb.DataNodeT) (*rpcfb.DataNodeT, error) {
	logger := e.lg

	if dataNode.NodeId <= 0 {
		logger.Error("invalid data node id", zap.Int32("node-id", dataNode.NodeId))
		return nil, errors.New("invalid data node id")
	}

	key := dataNodePath(dataNode.NodeId)
	value := fbutil.Marshal(dataNode)
	defer mcache.Free(value)

	_, err := e.Put(key, value, false)
	if err != nil {
		logger.Error("failed to save data node", zap.Int32("node-id", dataNode.NodeId), zap.Error(err))
		return nil, errors.Wrap(err, "save data node")
	}

	return dataNode, nil
}

// ForEachDataNode calls the given function for every data node in the storage.
// If f returns an error, the iteration is stopped and the error is returned.
func (e *Endpoint) ForEachDataNode(f func(dataNode *rpcfb.DataNodeT) error) error {
	var startID int32 = 1
	for startID > 0 {
		nextID, err := e.forEachDataNodeLimited(f, startID, _dataNodeRangeLimit)
		if err != nil {
			return err
		}
		startID = nextID
	}
	return nil
}

func (e *Endpoint) forEachDataNodeLimited(f func(dataNode *rpcfb.DataNodeT) error, startID int32, limit int64) (nextID int32, err error) {
	logger := e.lg

	startKey := dataNodePath(startID)
	kvs, err := e.GetByRange(kv.Range{StartKey: startKey, EndKey: e.endDataNodePath()}, limit)
	if err != nil {
		logger.Error("failed to get data nodes", zap.Int32("start-id", startID), zap.Int64("limit", limit), zap.Error(err))
		return 0, errors.Wrap(err, "get data nodes")
	}

	for _, keyValue := range kvs {
		dataNode := rpcfb.GetRootAsDataNode(keyValue.Value, 0).UnPack()
		nextID = dataNode.NodeId + 1
		err = f(dataNode)
		if err != nil {
			return 0, err
		}
	}

	// return 0 if no more data nodes
	if int64(len(kvs)) < limit {
		nextID = 0
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
