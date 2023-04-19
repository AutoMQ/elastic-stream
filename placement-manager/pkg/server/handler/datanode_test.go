package handler

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
)

func TestHandler_Heartbeat(t *testing.T) {
	re := require.New(t)

	// send heartbeats to a pm node which is not leader
	h, closeFunc := startSbpHandler(t, nil, false)
	defer closeFunc()

	req := &protocol.HeartbeatRequest{HeartbeatRequestT: rpcfb.HeartbeatRequestT{
		ClientRole: rpcfb.ClientRoleCLIENT_ROLE_DATA_NODE,
		DataNode: &rpcfb.DataNodeT{
			NodeId:        42,
			AdvertiseAddr: fmt.Sprintf("addr-%d", 42),
		}}}
	resp := &protocol.HeartbeatResponse{}
	h.Heartbeat(req, resp)

	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
}
