package handler

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	sbpClient "github.com/AutoMQ/placement-manager/pkg/sbp/client"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
)

func TestListRanges(t *testing.T) {
	re := require.New(t)

	h, closeFunc := startSbpHandler(t, nil, true)
	defer closeFunc()

	preHeartbeat(t, h, 0)
	preHeartbeat(t, h, 1)
	preHeartbeat(t, h, 2)
	preCreateStreams(t, h, 3, 3)

	// test list range by stream id
	req := &protocol.ListRangesRequest{ListRangesRequestT: rpcfb.ListRangesRequestT{
		RangeCriteria: []*rpcfb.RangeCriteriaT{
			{StreamId: 0, NodeId: -1},
			{StreamId: 1, NodeId: -1},
			{StreamId: 2, NodeId: -1},
		},
	}}
	resp := &protocol.ListRangesResponse{}
	h.ListRanges(req, resp)

	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
	re.Len(resp.ListResponses, 3)
	for i, result := range resp.ListResponses {
		re.Equal(result.RangeCriteria, req.RangeCriteria[i])
		re.Equal(rpcfb.ErrorCodeOK, result.Status.Code)
		re.Len(result.Ranges, 1)
	}

	// test list range by data node id
	req = &protocol.ListRangesRequest{ListRangesRequestT: rpcfb.ListRangesRequestT{
		RangeCriteria: []*rpcfb.RangeCriteriaT{
			{StreamId: -1, NodeId: 0},
			{StreamId: -1, NodeId: 1},
			{StreamId: -1, NodeId: 2},
		},
	}}
	resp = &protocol.ListRangesResponse{}
	h.ListRanges(req, resp)

	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
	re.Len(resp.ListResponses, 3)
	for i, result := range resp.ListResponses {
		re.Equal(result.RangeCriteria, req.RangeCriteria[i])
		re.Equal(rpcfb.ErrorCodeOK, result.Status.Code)
		re.Len(result.Ranges, 3)
	}

	// test list range by stream id and data node id
	req = &protocol.ListRangesRequest{ListRangesRequestT: rpcfb.ListRangesRequestT{
		RangeCriteria: []*rpcfb.RangeCriteriaT{
			{StreamId: 0, NodeId: 0},
			{StreamId: 1, NodeId: 1},
			{StreamId: 2, NodeId: 2},
		},
	}}
	resp = &protocol.ListRangesResponse{}
	h.ListRanges(req, resp)

	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
	re.Len(resp.ListResponses, 3)
	for i, result := range resp.ListResponses {
		re.Equal(result.RangeCriteria, req.RangeCriteria[i])
		re.Equal(rpcfb.ErrorCodeOK, result.Status.Code)
		re.Len(result.Ranges, 1)
	}
}

func TestSealRanges(t *testing.T) {
	type want struct {
		endOffset int64
		wantErr   bool
		errCode   rpcfb.ErrorCode
	}
	tests := []struct {
		name       string
		endOffsetF func(rangeIndex int32, addr sbpClient.Address) int64
		want       want
	}{
		{
			name: "no data node responded",
			endOffsetF: func(rangeIndex int32, addr sbpClient.Address) int64 {
				return -1
			},
			want: want{wantErr: true, errCode: rpcfb.ErrorCodePM_SEAL_RANGE_NO_DN_RESPONDED},
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			h, closeFunc := startSbpHandler(t, mockSbpClient{tt.endOffsetF}, true)
			defer closeFunc()

			preHeartbeat(t, h, 0)
			preHeartbeat(t, h, 1)
			preHeartbeat(t, h, 2)
			preCreateStreams(t, h, 1, 3)

			req := &protocol.SealRangesRequest{SealRangesRequestT: rpcfb.SealRangesRequestT{
				Entries: []*rpcfb.SealRangeEntryT{{Range: &rpcfb.RangeIdT{StreamId: 0, RangeIndex: 0}}},
			}}
			resp := &protocol.SealRangesResponse{}
			h.SealRanges(req, resp)

			re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
			re.Len(resp.SealResponses, 1)
			if tt.want.wantErr {
				re.Equal(tt.want.errCode, resp.SealResponses[0].Status.Code)
			} else {
				re.Equal(rpcfb.ErrorCodeOK, resp.SealResponses[0].Status.Code)
			}
			re.Equal(tt.want.endOffset, resp.SealResponses[0].Range.StartOffset)
			re.Equal(int64(-1), resp.SealResponses[0].Range.EndOffset)

			lReq := &protocol.ListRangesRequest{ListRangesRequestT: rpcfb.ListRangesRequestT{
				RangeCriteria: []*rpcfb.RangeCriteriaT{
					{StreamId: 0, NodeId: -1},
				},
			}}
			lResp := &protocol.ListRangesResponse{}
			h.ListRanges(lReq, lResp)
			re.Equal(rpcfb.ErrorCodeOK, lResp.Status.Code)
			re.Len(lResp.ListResponses, 1)
			re.Equal(rpcfb.ErrorCodeOK, lResp.ListResponses[0].Status.Code)

			if tt.want.wantErr {
				re.Len(lResp.ListResponses[0].Ranges, 1)
				re.Equal(int64(0), lResp.ListResponses[0].Ranges[0].StartOffset)
				re.Equal(int64(-1), lResp.ListResponses[0].Ranges[0].EndOffset)
				return
			}
			re.Len(lResp.ListResponses[0].Ranges, 2)
			re.Equal(int64(0), lResp.ListResponses[0].Ranges[0].StartOffset)
			re.Equal(tt.want.endOffset, lResp.ListResponses[0].Ranges[0].EndOffset)
			re.Equal(tt.want.endOffset, lResp.ListResponses[0].Ranges[1].StartOffset)
			re.Equal(int64(-1), lResp.ListResponses[0].Ranges[1].EndOffset)
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

func preCreateStreams(tb testing.TB, h *Handler, num int, replicaNum int8) {
	re := require.New(tb)

	req := &protocol.CreateStreamsRequest{CreateStreamsRequestT: rpcfb.CreateStreamsRequestT{
		Streams: make([]*rpcfb.StreamT, num),
	}}
	for i := 0; i < num; i++ {
		req.Streams[i] = &rpcfb.StreamT{ReplicaNum: replicaNum}
	}
	resp := &protocol.CreateStreamsResponse{}
	h.CreateStreams(req, resp)

	re.Equal(resp.Status.Code, rpcfb.ErrorCodeOK)
}
