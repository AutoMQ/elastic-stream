package cache

import (
	"time"

	cmap "github.com/orcaman/concurrent-map/v2"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
)

// Cache is the cache for all metadata.
type Cache struct {
	dataNodes cmap.ConcurrentMap[int32, *DataNode]
}

// NewCache creates a new Cache.
func NewCache() *Cache {
	return &Cache{
		dataNodes: cmap.NewWithCustomShardingFunction[int32, *DataNode](func(key int32) uint32 { return uint32(key) }),
	}
}

// Reset resets the cache.
func (c *Cache) Reset() {
	// No need to reset data nodes, as they will be updated by heartbeat.
}

// DataNode is the cache for DataNodeT and its status.
type DataNode struct {
	rpcfb.DataNodeT
	LastActiveTime time.Time
	Metrics        *rpcfb.DataNodeMetricsT
}

// Score returns the score of the data node.
func (n *DataNode) Score() (score int) {
	// TODO more intelligent score
	if n.Metrics == nil {
		return
	}

	const (
		KB = 1024
		MB = 1024 * KB
		GB = 1024 * MB
	)

	score += 10000
	score -= int(10 * n.Metrics.DiskInRate / (100 * MB))
	score -= int(10 * n.Metrics.DiskOutRate / (100 * MB))
	score += int(10 * n.Metrics.DiskFreeSpace / (50 * GB))
	score -= int(10 * n.Metrics.DiskUnindexedDataSize / (100 * MB))
	score -= int(10 * n.Metrics.MemoryUsed / (1 * GB))
	score -= int(10 * n.Metrics.UringTaskRate / 1024)
	score -= int(10 * n.Metrics.UringInflightTaskCnt / 128)
	score -= int(10 * n.Metrics.UringPendingTaskCnt / 1024)
	score -= int(10 * n.Metrics.UringTaskAvgLatency / 10)
	score -= int(10 * n.Metrics.NetworkAppendRate / 256)
	score -= int(10 * n.Metrics.NetworkFetchRate / 256)
	score -= int(10 * n.Metrics.NetworkFailedAppendRate / 1)
	score -= int(10 * n.Metrics.NetworkFailedFetchRate / 1)
	score -= int(10 * n.Metrics.NetworkAppendAvgLatency / 1)
	score -= int(10 * n.Metrics.NetworkAppendAvgLatency / 1)
	score -= int(10 * n.Metrics.RangeMissingReplicaCnt / 2)
	score -= int(10 * n.Metrics.RangeActiveCnt / 10)

	return
}

// SaveDataNode saves a data node to the cache.
// It returns true if the data node is new or its info is updated.
// If its info is updated, the old value is returned.
func (c *Cache) SaveDataNode(node *DataNode) (updated bool, old rpcfb.DataNodeT) {
	_ = c.dataNodes.Upsert(node.NodeId, node, func(exist bool, valueInMap, newValue *DataNode) *DataNode {
		if exist {
			if !isDataNodeEqual(valueInMap.DataNodeT, newValue.DataNodeT) {
				updated = true
				valueInMap.DataNodeT = newValue.DataNodeT
				old = valueInMap.DataNodeT
			}
			valueInMap.LastActiveTime = newValue.LastActiveTime
			if newValue.Metrics != nil {
				valueInMap.Metrics = newValue.Metrics
			}
			return valueInMap
		}
		updated = true
		return newValue
	})
	return
}

// DataNode returns the data node by node ID.
// The returned value is nil if the data node is not found.
// The returned value should not be modified.
func (c *Cache) DataNode(nodeID int32) *DataNode {
	node, ok := c.dataNodes.Get(nodeID)
	if !ok {
		return nil
	}
	return node
}

// ActiveDataNodes returns all active data nodes.
func (c *Cache) ActiveDataNodes(timeout time.Duration) []*DataNode {
	nodes := make([]*DataNode, 0)
	c.dataNodes.IterCb(func(_ int32, node *DataNode) {
		if node.LastActiveTime.IsZero() || time.Since(node.LastActiveTime) > timeout {
			return
		}
		if node.Metrics != nil && node.Metrics.DiskFreeSpace == 0 {
			return
		}
		nodes = append(nodes, node)
	})
	return nodes
}

// DataNodeCount returns the count of data nodes in the cache.
func (c *Cache) DataNodeCount() int {
	return c.dataNodes.Count()
}

func isDataNodeEqual(a, b rpcfb.DataNodeT) bool {
	return a.NodeId == b.NodeId &&
		a.AdvertiseAddr == b.AdvertiseAddr
}
