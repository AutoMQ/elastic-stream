package cluster

import (
	"context"
	"testing"

	"github.com/brianvoe/gofakeit/v6"
	"github.com/stretchr/testify/require"
	"go.uber.org/zap"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/server/cluster/cache"
)

// TestRaftCluster_fillDataNodesInfo will fail if there are new fields in rpcfb.RangeServerT
func TestRaftCluster_fillDataNodesInfo(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	var node rpcfb.RangeServerT
	_ = gofakeit.New(1).Struct(&node)
	cluster := NewRaftCluster(context.Background(), nil, nil, zap.NewNop())
	cluster.cache.SaveDataNode(&cache.RangeServer{
		RangeServerT: node,
	})

	node2 := rpcfb.RangeServerT{
		ServerId: node.ServerId,
	}
	cluster.fillDataNodesInfo([]*rpcfb.RangeServerT{&node2})

	re.Equal(node, node2)
}

// Test_eraseDataNodesInfo will fail if there are new fields in rpcfb.RangeServerT
func Test_eraseDataNodesInfo(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	var node rpcfb.RangeServerT
	_ = gofakeit.New(1).Struct(&node)

	nodes := eraseDataNodesInfo([]*rpcfb.RangeServerT{&node})

	// `AdvertiseAddr` should not be copied
	node.AdvertiseAddr = ""
	re.Equal(node, *nodes[0])

	// returned nodes should be a copy
	node.AdvertiseAddr = "modified"
	re.Equal("", nodes[0].AdvertiseAddr)
}
