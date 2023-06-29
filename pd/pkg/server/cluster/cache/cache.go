package cache

import (
	"time"

	cmap "github.com/orcaman/concurrent-map/v2"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
)

// Cache is the cache for all metadata.
type Cache struct {
	rangeServers cmap.ConcurrentMap[int32, *RangeServer]
}

// NewCache creates a new Cache.
func NewCache() *Cache {
	return &Cache{
		rangeServers: cmap.NewWithCustomShardingFunction[int32, *RangeServer](func(key int32) uint32 { return uint32(key) }),
	}
}

// Reset resets the cache.
func (c *Cache) Reset() {
	// No need to reset range servers, as they will be updated by heartbeat.
}

// RangeServer is the cache for RangeServerT and its status.
type RangeServer struct {
	rpcfb.RangeServerT
	LastActiveTime time.Time
	Metrics        *rpcfb.RangeServerMetricsT
}

// Score returns the score of the range server.
func (rs *RangeServer) Score() (score int) {
	// TODO more intelligent score
	if rs.Metrics == nil {
		return
	}

	const (
		KB = 1024
		MB = 1024 * KB
		GB = 1024 * MB
	)

	score += 10000
	score -= int(10 * rs.Metrics.DiskInRate / (100 * MB))
	score -= int(10 * rs.Metrics.DiskOutRate / (100 * MB))
	score += int(10 * rs.Metrics.DiskFreeSpace / (50 * GB))
	score -= int(10 * rs.Metrics.DiskUnindexedDataSize / (100 * MB))
	score -= int(10 * rs.Metrics.MemoryUsed / (1 * GB))
	score -= int(10 * rs.Metrics.UringTaskRate / 1024)
	score -= int(10 * rs.Metrics.UringInflightTaskCnt / 128)
	score -= int(10 * rs.Metrics.UringPendingTaskCnt / 1024)
	score -= int(10 * rs.Metrics.UringTaskAvgLatency / 10)
	score -= int(10 * rs.Metrics.NetworkAppendRate / 256)
	score -= int(10 * rs.Metrics.NetworkFetchRate / 256)
	score -= int(10 * rs.Metrics.NetworkFailedAppendRate / 1)
	score -= int(10 * rs.Metrics.NetworkFailedFetchRate / 1)
	score -= int(10 * rs.Metrics.NetworkAppendAvgLatency / 1)
	score -= int(10 * rs.Metrics.NetworkAppendAvgLatency / 1)
	score -= int(10 * rs.Metrics.RangeMissingReplicaCnt / 2)
	score -= int(10 * rs.Metrics.RangeActiveCnt / 10)

	return
}

// SaveRangeServer saves a range server to the cache.
// It returns true if the range server is new or its info is updated.
// If its info is updated, the old value is returned.
func (c *Cache) SaveRangeServer(rangeServer *RangeServer) (updated bool, old rpcfb.RangeServerT) {
	_ = c.rangeServers.Upsert(rangeServer.ServerId, rangeServer, func(exist bool, valueInMap, newValue *RangeServer) *RangeServer {
		if exist {
			if !isRangeServerEqual(valueInMap.RangeServerT, newValue.RangeServerT) {
				updated = true
				valueInMap.RangeServerT = newValue.RangeServerT
				old = valueInMap.RangeServerT
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

// RangeServer returns the range server by server ID.
// The returned value is nil if the range server is not found.
// The returned value should not be modified.
func (c *Cache) RangeServer(serverID int32) *RangeServer {
	rangeServer, ok := c.rangeServers.Get(serverID)
	if !ok {
		return nil
	}
	return rangeServer
}

// ActiveRangeServers returns all active range servers.
func (c *Cache) ActiveRangeServers(timeout time.Duration) []*RangeServer {
	rangeServers := make([]*RangeServer, 0)
	c.rangeServers.IterCb(func(_ int32, rangeServer *RangeServer) {
		if rangeServer.LastActiveTime.IsZero() || time.Since(rangeServer.LastActiveTime) > timeout {
			return
		}
		if rangeServer.Metrics != nil && rangeServer.Metrics.DiskFreeSpace == 0 {
			return
		}
		rangeServers = append(rangeServers, rangeServer)
	})
	return rangeServers
}

// RangeServerCount returns the count of range servers in the cache.
func (c *Cache) RangeServerCount() int {
	return c.rangeServers.Count()
}

func isRangeServerEqual(a, b rpcfb.RangeServerT) bool {
	return a.ServerId == b.ServerId &&
		a.AdvertiseAddr == b.AdvertiseAddr
}
