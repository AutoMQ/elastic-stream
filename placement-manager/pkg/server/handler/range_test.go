package handler

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
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
			{StreamId: 0, DataNode: &rpcfb.DataNodeT{NodeId: -1}},
			{StreamId: 1, DataNode: &rpcfb.DataNodeT{NodeId: -1}},
			{StreamId: 2, DataNode: &rpcfb.DataNodeT{NodeId: -1}},
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
			{StreamId: -1, DataNode: &rpcfb.DataNodeT{NodeId: 0}},
			{StreamId: -1, DataNode: &rpcfb.DataNodeT{NodeId: 1}},
			{StreamId: -1, DataNode: &rpcfb.DataNodeT{NodeId: 2}},
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
			{StreamId: 0, DataNode: &rpcfb.DataNodeT{NodeId: 0}},
			{StreamId: 1, DataNode: &rpcfb.DataNodeT{NodeId: 1}},
			{StreamId: 2, DataNode: &rpcfb.DataNodeT{NodeId: 2}},
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
		writableRange *rpcfb.RangeT
		ranges        []*rpcfb.RangeT
		wantErr       bool
		errCode       rpcfb.ErrorCode
	}
	tests := []struct {
		name  string
		entry *rpcfb.SealRangeEntryT
		want  want
	}{
		{
			name: "seal and renew",
			entry: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeIdT{RangeIndex: 1},
				End:   42,
				Renew: true,
			},
			want: want{
				writableRange: &rpcfb.RangeT{RangeIndex: 2, StartOffset: 42, EndOffset: -1},
				ranges: []*rpcfb.RangeT{
					{},
					{RangeIndex: 1, EndOffset: 42},
					{RangeIndex: 2, StartOffset: 42, EndOffset: -1},
				},
			},
		},
		{
			name: "seal only",
			entry: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeIdT{RangeIndex: 1},
				End:   42,
			},
			want: want{
				ranges: []*rpcfb.RangeT{
					{},
					{RangeIndex: 1, EndOffset: 42},
				},
			},
		},
	}
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
			preSealRange(t, h, &rpcfb.RangeIdT{}, 0)

			// seal ranges
			req := &protocol.SealRangesRequest{SealRangesRequestT: rpcfb.SealRangesRequestT{
				Entries: []*rpcfb.SealRangeEntryT{tt.entry},
			}}
			resp := &protocol.SealRangesResponse{}
			h.SealRanges(req, resp)

			// check seal response
			re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
			re.Len(resp.SealResponses, 1)
			if tt.want.wantErr {
				re.Equal(tt.want.errCode, resp.SealResponses[0].Status.Code)
			} else {
				re.Equal(rpcfb.ErrorCodeOK, resp.SealResponses[0].Status.Code)
			}

			if resp.SealResponses[0].Range != nil {
				resp.SealResponses[0].Range.ReplicaNodes = nil // no need check replica nodes
			}
			re.Equal(tt.want.writableRange, resp.SealResponses[0].Range)

			// list ranges
			lReq := &protocol.ListRangesRequest{ListRangesRequestT: rpcfb.ListRangesRequestT{
				RangeCriteria: []*rpcfb.RangeCriteriaT{
					{StreamId: 0, DataNode: &rpcfb.DataNodeT{NodeId: -1}},
				},
			}}
			lResp := &protocol.ListRangesResponse{}
			h.ListRanges(lReq, lResp)
			re.Equal(rpcfb.ErrorCodeOK, lResp.Status.Code)
			re.Len(lResp.ListResponses, 1)
			re.Equal(rpcfb.ErrorCodeOK, lResp.ListResponses[0].Status.Code)

			// check list range response
			for _, rangeT := range lResp.ListResponses[0].Ranges {
				rangeT.ReplicaNodes = nil // no need check replica nodes
			}
			re.Equal(tt.want.ranges, lResp.ListResponses[0].Ranges)
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

func preSealRange(tb testing.TB, h *Handler, rangeID *rpcfb.RangeIdT, end int64) {
	re := require.New(tb)

	req := &protocol.SealRangesRequest{SealRangesRequestT: rpcfb.SealRangesRequestT{
		Entries: []*rpcfb.SealRangeEntryT{{
			Type:  rpcfb.SealTypePLACEMENT_MANAGER,
			Range: rangeID,
			End:   end,
			Renew: true,
		}},
	}}
	resp := &protocol.SealRangesResponse{}
	h.SealRanges(req, resp)

	re.Equal(resp.Status.Code, rpcfb.ErrorCodeOK)
	re.Equal(resp.SealResponses[0].Status.Code, rpcfb.ErrorCodeOK)
	resp.SealResponses[0].Range.ReplicaNodes = nil // no need check replica nodes
	re.Equal(&rpcfb.RangeT{
		StreamId:    rangeID.StreamId,
		RangeIndex:  rangeID.RangeIndex + 1,
		StartOffset: end,
		EndOffset:   -1,
	}, resp.SealResponses[0].Range)
}
