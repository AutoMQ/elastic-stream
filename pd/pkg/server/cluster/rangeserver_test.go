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

	var rangeServer rpcfb.RangeServerT
	_ = gofakeit.New(1).Struct(&rangeServer)
	cluster := NewRaftCluster(context.Background(), nil, nil, zap.NewNop())
	cluster.cache.SaveRangeServer(&cache.RangeServer{
		RangeServerT: rangeServer,
	})

	rangeServer2 := rpcfb.RangeServerT{
		ServerId: rangeServer.ServerId,
	}
	cluster.fillRangeServersInfo([]*rpcfb.RangeServerT{&rangeServer2})

	re.Equal(rangeServer, rangeServer2)
}

// Test_eraseRangeServersInfo will fail if there are new fields in rpcfb.RangeServerT
func Test_eraseRangeServersInfo(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	var rangeServer rpcfb.RangeServerT
	_ = gofakeit.New(1).Struct(&rangeServer)

	servers := eraseRangeServersInfo([]*rpcfb.RangeServerT{&rangeServer})

	// `AdvertiseAddr` should not be copied
	rangeServer.AdvertiseAddr = ""
	re.Equal(rangeServer, *servers[0])

	// returned servers should be a copy
	rangeServer.AdvertiseAddr = "modified"
	re.Equal("", servers[0].AdvertiseAddr)
}
