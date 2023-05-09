package cluster

import (
	"context"
	"testing"

	"github.com/brianvoe/gofakeit/v6"
	"github.com/stretchr/testify/require"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster/cache"
)

// TestRaftCluster_fillDataNodesInfo will fail if there are new fields in rpcfb.DataNodeT
func TestRaftCluster_fillDataNodesInfo(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	var node rpcfb.DataNodeT
	_ = gofakeit.New(1).Struct(&node)
	cluster := NewRaftCluster(context.Background(), nil, zap.NewNop())
	cluster.cache.SaveDataNode(&cache.DataNode{
		DataNodeT: node,
	})

	node2 := rpcfb.DataNodeT{
		NodeId: node.NodeId,
	}
	cluster.fillDataNodesInfo([]*rpcfb.DataNodeT{&node2})

	re.Equal(node, node2)
}

// Test_eraseDataNodesInfo will fail if there are new fields in rpcfb.DataNodeT
func Test_eraseDataNodesInfo(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	var node rpcfb.DataNodeT
	_ = gofakeit.New(1).Struct(&node)

	nodes := eraseDataNodesInfo([]*rpcfb.DataNodeT{&node})

	// `AdvertiseAddr` should not be copied
	node.AdvertiseAddr = ""
	re.Equal(node, *nodes[0])

	// returned nodes should be a copy
	node.AdvertiseAddr = "modified"
	re.Equal("", nodes[0].AdvertiseAddr)
}
