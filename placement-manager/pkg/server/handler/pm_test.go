package handler

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
)

func TestHandler_DescribePMCluster(t *testing.T) {
	re := require.New(t)

	h, closeFunc := startSbpHandler(t, nil, true)
	defer closeFunc()

	req := &protocol.DescribePMClusterRequest{DescribePlacementManagerClusterRequestT: rpcfb.DescribePlacementManagerClusterRequestT{
		DataNode: &rpcfb.DataNodeT{
			NodeId:        42,
			AdvertiseAddr: fmt.Sprintf("addr-%d", 42),
		},
	}}
	resp := &protocol.DescribePMClusterResponse{}
	h.DescribePMCluster(req, resp)

	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
	re.Len(resp.Cluster.Nodes, 1)
	re.True(resp.Cluster.Nodes[0].IsLeader)
	re.Equal("test-member-name", resp.Cluster.Nodes[0].Name)
	re.Equal("test-member-sbp-addr", resp.Cluster.Nodes[0].AdvertiseAddr)
}
