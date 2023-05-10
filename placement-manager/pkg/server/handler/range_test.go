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
		writableRange *rpcfb.RangeT
		ranges        []*rpcfb.RangeT
		wantErr       bool
		errCode       rpcfb.ErrorCode
	}
	tests := []struct {
		name    string
		preSeal *rpcfb.SealRangeEntryT
		seal    *rpcfb.SealRangeEntryT
		want    want
	}{
		{
			name:    "normal case (seal a writable range; renew; last range is writable)",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{EndOffset: 42}, Renew: true},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{RangeIndex: 1, EndOffset: 84},
				Renew: true,
			},
			want: want{
				writableRange: &rpcfb.RangeT{RangeIndex: 2, StartOffset: 84, EndOffset: -1},
				ranges: []*rpcfb.RangeT{
					{EndOffset: 42},
					{RangeIndex: 1, StartOffset: 42, EndOffset: 84},
					{RangeIndex: 2, StartOffset: 84, EndOffset: -1},
				},
			},
		},

		{
			name:    "seal a writable range; renew; last range is writable",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{}, Renew: true},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{RangeIndex: 1, EndOffset: 42},
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
			name:    "seal a writable range; not renew; last range is writable",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{}, Renew: true},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{RangeIndex: 1, EndOffset: 42},
			},
			want: want{
				ranges: []*rpcfb.RangeT{
					{},
					{RangeIndex: 1, EndOffset: 42},
				},
			},
		},

		{
			name:    "seal a sealed range; renew; last range is writable",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{}, Renew: true},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{EndOffset: 42},
				Renew: true,
			},
			want: want{
				writableRange: &rpcfb.RangeT{RangeIndex: 1, EndOffset: -1},
				ranges: []*rpcfb.RangeT{
					{},
					{RangeIndex: 1, EndOffset: -1},
				},
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_ALREADY_SEALED,
			},
		},
		{
			name:    "seal a sealed range; renew; last range is sealed",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{}},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{EndOffset: 42},
				Renew: true,
			},
			want: want{
				writableRange: &rpcfb.RangeT{RangeIndex: 1, EndOffset: -1},
				ranges: []*rpcfb.RangeT{
					{},
					{RangeIndex: 1, EndOffset: -1},
				},
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_ALREADY_SEALED,
			},
		},
		{
			name:    "seal a sealed range; not renew; last range is writable",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{}, Renew: true},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{EndOffset: 42},
			},
			want: want{
				ranges: []*rpcfb.RangeT{
					{},
					{RangeIndex: 1, EndOffset: -1},
				},
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_ALREADY_SEALED,
			},
		},
		{
			name:    "seal a sealed range; not renew; last range is sealed",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{}},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{EndOffset: 42},
			},
			want: want{
				ranges: []*rpcfb.RangeT{
					{},
				},
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_ALREADY_SEALED,
			},
		},

		{
			name:    "seal a non-exist range; renew; last range is writable",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{}, Renew: true},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{RangeIndex: 2, EndOffset: 42},
				Renew: true,
			},
			want: want{
				writableRange: &rpcfb.RangeT{RangeIndex: 1, EndOffset: -1},
				ranges: []*rpcfb.RangeT{
					{},
					{RangeIndex: 1, EndOffset: -1},
				},
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_NOT_FOUND,
			},
		},
		{
			name:    "seal a non-exist range; renew; last range is sealed",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{}},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{RangeIndex: 2, EndOffset: 42},
				Renew: true,
			},
			want: want{
				writableRange: &rpcfb.RangeT{RangeIndex: 1, EndOffset: -1},
				ranges: []*rpcfb.RangeT{
					{},
					{RangeIndex: 1, EndOffset: -1},
				},
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_NOT_FOUND,
			},
		},
		{
			name:    "seal a non-exist range; not renew; last range is writable",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{}, Renew: true},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{RangeIndex: 2, EndOffset: 42},
			},
			want: want{
				ranges: []*rpcfb.RangeT{
					{},
					{RangeIndex: 1, EndOffset: -1},
				},
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_NOT_FOUND,
			},
		},
		{
			name:    "seal a non-exist range; not renew; last range is sealed",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{}},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{RangeIndex: 2, EndOffset: 42},
			},
			want: want{
				ranges: []*rpcfb.RangeT{
					{},
				},
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_NOT_FOUND,
			},
		},

		{
			name:    "invalid type",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{}},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypeDATA_NODE,
				Range: &rpcfb.RangeT{EndOffset: 42},
				Renew: true,
			},
			want: want{
				writableRange: nil,
				ranges: []*rpcfb.RangeT{
					{},
				},
				wantErr: true,
				errCode: rpcfb.ErrorCodeBAD_REQUEST,
			},
		},
		{
			name:    "stream not exist",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{}},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{StreamId: 1, RangeIndex: 1, EndOffset: 42},
				Renew: true,
			},
			want: want{
				writableRange: nil,
				ranges: []*rpcfb.RangeT{
					{},
				},
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_NOT_FOUND,
			},
		},
		{
			name:    "invalid end offset",
			preSeal: &rpcfb.SealRangeEntryT{Range: &rpcfb.RangeT{EndOffset: 84}, Renew: true},
			seal: &rpcfb.SealRangeEntryT{
				Type:  rpcfb.SealTypePLACEMENT_MANAGER,
				Range: &rpcfb.RangeT{RangeIndex: 1, EndOffset: 42},
				Renew: true,
			},
			want: want{
				writableRange: nil,
				ranges: []*rpcfb.RangeT{
					{EndOffset: 84},
					{RangeIndex: 1, StartOffset: 84, EndOffset: -1},
				},
				wantErr: true,
				errCode: rpcfb.ErrorCodeBAD_REQUEST,
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
			preSealRange(t, h, tt.preSeal)

			// seal ranges
			req := &protocol.SealRangesRequest{SealRangesRequestT: rpcfb.SealRangesRequestT{
				Entries: []*rpcfb.SealRangeEntryT{tt.seal},
			}}
			resp := &protocol.SealRangesResponse{}
			h.SealRanges(req, resp)

			// check seal response
			re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
			re.Len(resp.Results, 1)
			if tt.want.wantErr {
				re.Equal(tt.want.errCode, resp.Results[0].Status.Code)
			} else {
				re.Equal(rpcfb.ErrorCodeOK, resp.Results[0].Status.Code)
			}

			if resp.Results[0].Range != nil {
				resp.Results[0].Range.ReplicaNodes = nil // no need check replica nodes
			}
			re.Equal(tt.want.writableRange, resp.Results[0].Range)

			// list ranges
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

func preSealRange(tb testing.TB, h *Handler, entry *rpcfb.SealRangeEntryT) {
	re := require.New(tb)

	entry.Type = rpcfb.SealTypePLACEMENT_MANAGER
	req := &protocol.SealRangesRequest{SealRangesRequestT: rpcfb.SealRangesRequestT{
		Entries: []*rpcfb.SealRangeEntryT{entry},
	}}
	resp := &protocol.SealRangesResponse{}
	h.SealRanges(req, resp)

	re.Equal(resp.Status.Code, rpcfb.ErrorCodeOK)
	re.Equal(resp.Results[0].Status.Code, rpcfb.ErrorCodeOK)
	if entry.Renew {
		resp.Results[0].Range.ReplicaNodes = nil // no need check replica nodes
		re.Equal(&rpcfb.RangeT{
			StreamId:    entry.Range.StreamId,
			RangeIndex:  entry.Range.RangeIndex + 1,
			StartOffset: entry.Range.EndOffset,
			EndOffset:   -1,
		}, resp.Results[0].Range)
	}
}
