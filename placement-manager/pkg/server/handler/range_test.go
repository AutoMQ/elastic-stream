package handler

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
)

func TestListRange(t *testing.T) {
	re := require.New(t)

	h, closeFunc := startSbpHandler(t, nil, true)
	defer closeFunc()

	preHeartbeat(t, h, 0)
	preHeartbeat(t, h, 1)
	preHeartbeat(t, h, 2)
	preCreateStreams(t, h, 3, 3)

	// test list range by stream id
	req := &protocol.ListRangeRequest{ListRangeRequestT: rpcfb.ListRangeRequestT{
		Criteria: &rpcfb.ListRangeCriteriaT{StreamId: 0, NodeId: -1},
	}}
	resp := &protocol.ListRangeResponse{}
	h.ListRange(req, resp)

	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
	re.Len(resp.Ranges, 1)

	// test list range by data node id
	req = &protocol.ListRangeRequest{ListRangeRequestT: rpcfb.ListRangeRequestT{
		Criteria: &rpcfb.ListRangeCriteriaT{StreamId: -1, NodeId: 0},
	}}
	resp = &protocol.ListRangeResponse{}
	h.ListRange(req, resp)

	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
	re.Len(resp.Ranges, 3)

	// test list range by stream id and data node id
	req = &protocol.ListRangeRequest{ListRangeRequestT: rpcfb.ListRangeRequestT{
		Criteria: &rpcfb.ListRangeCriteriaT{StreamId: 0, NodeId: 0},
	}}
	resp = &protocol.ListRangeResponse{}
	h.ListRange(req, resp)

	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
	re.Len(resp.Ranges, 1)
}

func TestSealRange(t *testing.T) {
	type want struct {
		writableRange *rpcfb.RangeT
		ranges        []*rpcfb.RangeT
		wantErr       bool
		errCode       rpcfb.ErrorCode
	}
	tests := []struct {
		name    string
		preSeal *rpcfb.SealRangeRequestT
		seal    *rpcfb.SealRangeRequestT
		want    want
	}{}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			h, closeFunc := startSbpHandler(t, nil, true)
			defer closeFunc()

			// prepare
			preHeartbeat(t, h, 0)
			preHeartbeat(t, h, 1)
			preHeartbeat(t, h, 2)
			preCreateStreams(t, h, 1, 3)
			preSealRange(t, h, tt.preSeal)

			// seal ranges
			req := &protocol.SealRangeRequest{SealRangeRequestT: *tt.seal}
			resp := &protocol.SealRangeResponse{}
			h.SealRange(req, resp)

			// check seal response
			re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
			if tt.want.wantErr {
				re.Equal(tt.want.errCode, resp.Status.Code)
			} else {
				re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
			}

			if resp.Range != nil {
				resp.Range.Nodes = nil // no need check replica nodes
			}
			re.Equal(tt.want.writableRange, resp.Range)

			// list ranges
			lReq := &protocol.ListRangeRequest{ListRangeRequestT: rpcfb.ListRangeRequestT{
				Criteria: &rpcfb.ListRangeCriteriaT{StreamId: 0, NodeId: -1},
			}}
			lResp := &protocol.ListRangeResponse{}
			h.ListRange(lReq, lResp)
			re.Equal(rpcfb.ErrorCodeOK, lResp.Status.Code)

			// check list range response
			for _, rangeT := range lResp.Ranges {
				rangeT.Nodes = nil // no need check replica nodes
			}
			re.Equal(tt.want.ranges, lResp.Ranges)
		})
	}
}

func preHeartbeat(tb testing.TB, h *Handler, nodeID int32) {
	re := require.New(tb)

	req := &protocol.HeartbeatRequest{HeartbeatRequestT: rpcfb.HeartbeatRequestT{
		ClientRole: rpcfb.ClientRoleCLIENT_ROLE_DATA_NODE,
		DataNode: &rpcfb.DataNodeT{
			NodeId:        nodeID,
			AdvertiseAddr: fmt.Sprintf("addr-%d", nodeID),
		}}}
	resp := &protocol.HeartbeatResponse{}

	h.Heartbeat(req, resp)
	re.Equal(resp.Status.Code, rpcfb.ErrorCodeOK)
}

func preCreateStreams(tb testing.TB, h *Handler, num int, replica int8) {
	re := require.New(tb)

	for i := 0; i < num; i++ {
		req := &protocol.CreateStreamRequest{CreateStreamRequestT: rpcfb.CreateStreamRequestT{
			Stream: &rpcfb.StreamT{Replica: replica},
		}}
		resp := &protocol.CreateStreamResponse{}

		h.CreateStream(req, resp)
		re.Equal(resp.Status.Code, rpcfb.ErrorCodeOK)
	}
}

func preSealRange(tb testing.TB, h *Handler, r *rpcfb.SealRangeRequestT) {
	re := require.New(tb)

	r.Kind = rpcfb.SealKindPLACEMENT_MANAGER
	req := &protocol.SealRangeRequest{SealRangeRequestT: *r}
	resp := &protocol.SealRangeResponse{}
	h.SealRange(req, resp)

	re.Equal(resp.Status.Code, rpcfb.ErrorCodeOK)
}
