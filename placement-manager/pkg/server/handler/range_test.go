package handler

import (
	"fmt"
	"sort"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
)

func TestHandler_ListRange(t *testing.T) {
	var nodes = []*rpcfb.DataNodeT{{NodeId: 0}, {NodeId: 1}, {NodeId: 2}}
	type args struct {
		StreamID int64
		NodeID   int32
	}
	tests := []struct {
		name string
		args args
		want []*rpcfb.RangeT
	}{
		{
			name: "list range by stream id",
			args: args{StreamID: 1, NodeID: -1},
			want: []*rpcfb.RangeT{
				{StreamId: 1, Index: 0, Start: 0, End: 42, Nodes: nodes},
				{StreamId: 1, Index: 1, Start: 42, End: -1, Nodes: nodes},
			},
		},
		{
			name: "list range by non-exist stream id",
			args: args{StreamID: 10, NodeID: -1},
			want: []*rpcfb.RangeT{},
		},
		{
			name: "list range by node id",
			args: args{StreamID: -1, NodeID: 1},
			want: []*rpcfb.RangeT{
				{StreamId: 0, Index: 0, Start: 0, End: 42, Nodes: nodes},
				{StreamId: 0, Index: 1, Start: 42, End: -1, Nodes: nodes},
				{StreamId: 1, Index: 0, Start: 0, End: 42, Nodes: nodes},
				{StreamId: 1, Index: 1, Start: 42, End: -1, Nodes: nodes},
				{StreamId: 2, Index: 0, Start: 0, End: 42, Nodes: nodes},
				{StreamId: 2, Index: 1, Start: 42, End: -1, Nodes: nodes},
			},
		},
		{
			name: "list range by non-exist node id",
			args: args{StreamID: -1, NodeID: 10},
			want: []*rpcfb.RangeT{},
		},
		{
			name: "list range by stream id and node id",
			args: args{StreamID: 2, NodeID: 2},
			want: []*rpcfb.RangeT{
				{StreamId: 2, Index: 0, Start: 0, End: 42, Nodes: nodes},
				{StreamId: 2, Index: 1, Start: 42, End: -1, Nodes: nodes},
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

			// prepare: 3 nodes, 3 streams, 1 sealed range and 1 writable range per stream
			preHeartbeats(t, h, 0, 1, 2)
			streamIDs := preCreateStreams(t, h, 3, 3)
			re.Equal([]int64{0, 1, 2}, streamIDs)
			for _, id := range streamIDs {
				r := preNewRange(t, h, id, true, 42)
				re.Equal(int32(0), r.Index)
				r = preNewRange(t, h, id, false)
				re.Equal(int32(1), r.Index)
			}

			req := &protocol.ListRangeRequest{ListRangeRequestT: rpcfb.ListRangeRequestT{
				Criteria: &rpcfb.ListRangeCriteriaT{StreamId: tt.args.StreamID, NodeId: tt.args.NodeID},
			}}
			resp := &protocol.ListRangeResponse{}
			h.ListRange(req, resp)

			re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code, resp.Status.Message)
			for _, r := range resp.Ranges {
				// erase advertise addr
				for _, n := range r.Nodes {
					n.AdvertiseAddr = ""
				}
				// sort by node id
				sort.Slice(r.Nodes, func(i, j int) bool {
					return r.Nodes[i].NodeId < r.Nodes[j].NodeId
				})
			}
			re.Equal(tt.want, resp.Ranges)
		})
	}
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
			preHeartbeats(t, h, 0, 1, 2)
			preCreateStreams(t, h, 3, 1)
			// preSealRange(t, h, tt.preSeal)

			// seal ranges
			req := &protocol.SealRangeRequest{SealRangeRequestT: *tt.seal}
			resp := &protocol.SealRangeResponse{}
			h.SealRange(req, resp)

			// check seal response
			re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code, resp.Status.Message)
			if tt.want.wantErr {
				re.Equal(tt.want.errCode, resp.Status.Code)
			} else {
				re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code, resp.Status.Message)
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
			re.Equal(rpcfb.ErrorCodeOK, lResp.Status.Code, lResp.Status.Message)

			// check list range response
			for _, rangeT := range lResp.Ranges {
				rangeT.Nodes = nil // no need check replica nodes
			}
			re.Equal(tt.want.ranges, lResp.Ranges)
		})
	}
}

func preHeartbeats(tb testing.TB, h *Handler, nodeIDs ...int32) {
	for _, nodeID := range nodeIDs {
		preHeartbeat(tb, h, nodeID)
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
	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code, resp.Status.Message)
}

func preCreateStreams(tb testing.TB, h *Handler, replica int8, cnt int) (streamIDs []int64) {
	streamIDs = make([]int64, 0, cnt)
	for i := 0; i < cnt; i++ {
		stream := preCreateStream(tb, h, replica)
		streamIDs = append(streamIDs, stream.StreamId)
	}
	return
}

func preCreateStream(tb testing.TB, h *Handler, replica int8) *rpcfb.StreamT {
	re := require.New(tb)

	req := &protocol.CreateStreamRequest{CreateStreamRequestT: rpcfb.CreateStreamRequestT{
		Stream: &rpcfb.StreamT{Replica: replica},
	}}
	resp := &protocol.CreateStreamResponse{}

	h.CreateStream(req, resp)
	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code, resp.Status.Message)

	return resp.Stream
}

func preNewRange(tb testing.TB, h *Handler, streamID int64, sealed bool, length ...int64) (r *rpcfb.RangeT) {
	// NOT thread safe
	r = createRange(tb, h, streamID)
	if sealed {
		r = sealRange(tb, h, streamID, length[0])
	}

	return
}

func createRange(tb testing.TB, h *Handler, streamID int64) *rpcfb.RangeT {
	re := require.New(tb)

	r := getLastRange(tb, h, streamID)
	req := &protocol.CreateRangeRequest{CreateRangeRequestT: rpcfb.CreateRangeRequestT{
		Range: &rpcfb.RangeT{
			StreamId: streamID,
			Epoch:    r.Epoch,
			Index:    r.Index + 1,
			Start:    r.End,
		},
	}}
	resp := &protocol.CreateRangeResponse{}

	h.CreateRange(req, resp)
	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code, resp.Status.Message)

	return resp.Range
}

func sealRange(tb testing.TB, h *Handler, streamID int64, length int64) *rpcfb.RangeT {
	re := require.New(tb)

	r := getLastRange(tb, h, streamID)
	req := &protocol.SealRangeRequest{SealRangeRequestT: rpcfb.SealRangeRequestT{
		Kind: rpcfb.SealKindPLACEMENT_MANAGER,
		Range: &rpcfb.RangeT{
			StreamId: streamID,
			Epoch:    r.Epoch,
			Index:    r.Index,
			Start:    r.Start,
			End:      r.Start + length,
		},
	}}
	resp := &protocol.SealRangeResponse{}

	h.SealRange(req, resp)
	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code, resp.Status.Message)

	return resp.Range
}

func getLastRange(tb testing.TB, h *Handler, streamID int64) *rpcfb.RangeT {
	re := require.New(tb)

	req := &protocol.ListRangeRequest{ListRangeRequestT: rpcfb.ListRangeRequestT{
		Criteria: &rpcfb.ListRangeCriteriaT{StreamId: streamID, NodeId: -1},
	}}
	resp := &protocol.ListRangeResponse{}
	h.ListRange(req, resp)
	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code, resp.Status.Message)

	r := &rpcfb.RangeT{StreamId: streamID, Index: -1}
	for _, rr := range resp.Ranges {
		if rr.Index > r.Index {
			r = rr
		}
	}

	return r
}
