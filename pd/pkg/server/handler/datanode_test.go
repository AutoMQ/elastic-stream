package handler

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/sbp/protocol"
)

func TestHandler_Heartbeat(t *testing.T) {
	re := require.New(t)

	// send heartbeats to a pd node which is not leader
	h, closeFunc := startSbpHandler(t, nil, false)
	defer closeFunc()

	req := &protocol.HeartbeatRequest{HeartbeatRequestT: rpcfb.HeartbeatRequestT{
		ClientRole: rpcfb.ClientRoleCLIENT_ROLE_RANGE_SERVER,
		RangeServer: &rpcfb.RangeServerT{
			ServerId:      42,
			AdvertiseAddr: fmt.Sprintf("addr-%d", 42),
		}}}
	resp := &protocol.HeartbeatResponse{}
	h.Heartbeat(req, resp)

	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
}

func TestHandler_DescribePDCluster(t *testing.T) {
	re := require.New(t)

	h, closeFunc := startSbpHandler(t, nil, true)
	defer closeFunc()

	req := &protocol.DescribePDClusterRequest{DescribePlacementDriverClusterRequestT: rpcfb.DescribePlacementDriverClusterRequestT{
		RangeServer: &rpcfb.RangeServerT{
			ServerId:      42,
			AdvertiseAddr: fmt.Sprintf("addr-%d", 42),
		},
	}}
	resp := &protocol.DescribePDClusterResponse{}
	h.DescribePDCluster(req, resp)

	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
	re.Len(resp.Cluster.Nodes, 1)
	re.True(resp.Cluster.Nodes[0].IsLeader)
	re.Equal("test-member-name", resp.Cluster.Nodes[0].Name)
	re.Equal("test-member-sbp-addr", resp.Cluster.Nodes[0].AdvertiseAddr)
}
