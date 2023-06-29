package cache

import (
	"testing"

	"github.com/brianvoe/gofakeit/v6"
	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
)

// Test_isRangeServerEqual will fail if there are new fields in rpcfb.RangeServerT
func Test_isRangeServerEqual(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	var node1, node2 rpcfb.RangeServerT
	_ = gofakeit.New(1).Struct(&node1)
	_ = gofakeit.New(2).Struct(&node2)

	node2.ServerId = node1.ServerId
	node2.AdvertiseAddr = node1.AdvertiseAddr

	re.True(isRangeServerEqual(node1, node2))
}
