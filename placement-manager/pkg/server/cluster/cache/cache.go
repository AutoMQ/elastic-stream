package cache

import (
	"time"

	cmap "github.com/orcaman/concurrent-map/v2"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
)

// Cache is the cache for all metadata.
type Cache struct {
	writableRanges cmap.ConcurrentMap[int64, *Range]
	dataNodes      cmap.ConcurrentMap[int32, *DataNode]
}

// NewCache creates a new Cache.
func NewCache() *Cache {
	return &Cache{
		writableRanges: cmap.NewWithCustomShardingFunction[int64, *Range](func(key int64) uint32 { return uint32(key) }),
		dataNodes:      cmap.NewWithCustomShardingFunction[int32, *DataNode](func(key int32) uint32 { return uint32(key) }),
	}
}

// Reset resets the cache.
func (c *Cache) Reset() {
	c.writableRanges.Clear()
}

type Range struct {
	*rpcfb.RangeT
	// mu is a 1-element semaphore channel controlling access to seal range.
	// Write to lock it, and read to unlock.
	mu chan struct{}
}

func (r *Range) Mu() chan struct{} {
	return r.mu
}

// WritableRange returns the writable range of the stream.
func (c *Cache) WritableRange(streamID int64) *Range {
	return c.writableRanges.Upsert(streamID, nil, func(exist bool, valueInMap, _ *Range) *Range {
		if exist {
			return valueInMap
		}
		return &Range{
			mu: make(chan struct{}, 1),
		}
	})
}

// DataNode is the cache for DataNodeT and its status.
type DataNode struct {
	rpcfb.DataNodeT
	LastActiveTime time.Time
}

// SaveDataNode saves a data node to the cache.
// It returns true if the data node is updated.
func (c *Cache) SaveDataNode(node *DataNode) (updated bool) {
	_ = c.dataNodes.Upsert(node.NodeId, node, func(exist bool, valueInMap, newValue *DataNode) *DataNode {
		if exist {
			if !isDataNodeEqual(valueInMap.DataNodeT, newValue.DataNodeT) {
				updated = true
				valueInMap.DataNodeT = newValue.DataNodeT
			}
			valueInMap.LastActiveTime = newValue.LastActiveTime
			return valueInMap
		}
		updated = true
		return newValue
	})
	return updated
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
