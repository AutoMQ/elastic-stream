package cache

import (
	"testing"

	"github.com/brianvoe/gofakeit/v6"
	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
)

// Test_isDataNodeEqual will fail if there are new fields in rpcfb.DataNodeT
func Test_isDataNodeEqual(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	var node1, node2 rpcfb.DataNodeT
	_ = gofakeit.New(1).Struct(&node1)
	_ = gofakeit.New(2).Struct(&node2)

	node2.NodeId = node1.NodeId
	node2.AdvertiseAddr = node1.AdvertiseAddr

	re.True(isDataNodeEqual(node1, node2))
}
