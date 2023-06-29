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

// TestRaftCluster_fillRangeServersInfo will fail if there are new fields in rpcfb.RangeServerT
func TestRaftCluster_fillRangeServersInfo(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	var node rpcfb.RangeServerT
	_ = gofakeit.New(1).Struct(&node)
	cluster := NewRaftCluster(context.Background(), nil, nil, zap.NewNop())
	cluster.cache.SaveRangeServer(&cache.RangeServer{
		RangeServerT: node,
	})

	node2 := rpcfb.RangeServerT{
		ServerId: node.ServerId,
	}
	cluster.fillRangeServersInfo([]*rpcfb.RangeServerT{&node2})

	re.Equal(node, node2)
}

// Test_eraseRangeServersInfo will fail if there are new fields in rpcfb.RangeServerT
func Test_eraseRangeServersInfo(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	var node rpcfb.RangeServerT
	_ = gofakeit.New(1).Struct(&node)

	nodes := eraseRangeServersInfo([]*rpcfb.RangeServerT{&node})

	// `AdvertiseAddr` should not be copied
	node.AdvertiseAddr = ""
	re.Equal(node, *nodes[0])

	// returned nodes should be a copy
	node.AdvertiseAddr = "modified"
	re.Equal("", nodes[0].AdvertiseAddr)
}
