package handler

import (
	"fmt"
	"sort"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/sbp/protocol"
)

func TestHandler_ListRange(t *testing.T) {
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
				{StreamId: 1, Epoch: 1, Index: 0, Start: 0, End: 42},
				{StreamId: 1, Epoch: 2, Index: 1, Start: 42, End: -1},
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
				{StreamId: 0, Epoch: 1, Index: 0, Start: 0, End: 42},
				{StreamId: 0, Epoch: 2, Index: 1, Start: 42, End: -1},
				{StreamId: 1, Epoch: 1, Index: 0, Start: 0, End: 42},
				{StreamId: 1, Epoch: 2, Index: 1, Start: 42, End: -1},
				{StreamId: 2, Epoch: 1, Index: 0, Start: 0, End: 42},
				{StreamId: 2, Epoch: 2, Index: 1, Start: 42, End: -1},
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
				{StreamId: 2, Epoch: 1, Index: 0, Start: 0, End: 42},
				{StreamId: 2, Epoch: 2, Index: 1, Start: 42, End: -1},
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
				fmtNodes(r)
			}
			for _, r := range tt.want {
				fillRangeInfo(r)
			}
			re.Equal(tt.want, resp.Ranges)
		})
	}
}

type preRange struct {
	index int32
	start int64
	end   int64
}

func prepareRanges(t *testing.T, h *Handler, streamID int64, ranges []preRange) {
	re := require.New(t)
	for _, r := range ranges {
		var rr *rpcfb.RangeT
		if r.end == -1 {
			rr = preNewRange(t, h, streamID, false)
		} else {
			rr = preNewRange(t, h, streamID, true, r.end-r.start)
		}
		re.Equal(r.index, rr.Index)
		re.Equal(r.start, rr.Start)
		re.Equal(r.end, rr.End)
	}
}

func TestSealRange(t *testing.T) {
	type args struct {
		kind     rpcfb.SealKind
		epoch    int64
		streamID int64
		index    int32
		end      int64
	}
	type want struct {
		returned *rpcfb.RangeT
		wantErr  bool
		errCode  rpcfb.ErrorCode
		errMsg   string
		after    []*rpcfb.RangeT
	}
	tests := []struct {
		name    string
		prepare []preRange
		args    args
		want    want
	}{
		{
			name: "normal case",
			prepare: []preRange{
				{end: 42},
				{index: 1, start: 42, end: -1},
			},
			args: args{kind: rpcfb.SealKindPLACEMENT_DRIVER, epoch: 2, index: 1, end: 84},
			want: want{
				returned: &rpcfb.RangeT{Epoch: 2, Index: 1, Start: 42, End: 84},
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
					{Epoch: 2, Index: 1, Start: 42, End: 84},
				},
			},
		},
		{
			name: "stream not exist",
			prepare: []preRange{
				{end: 42},
				{index: 1, start: 42, end: -1},
			},
			args: args{kind: rpcfb.SealKindPLACEMENT_DRIVER, epoch: 2, streamID: 1, index: 1, end: 84},
			want: want{
				wantErr: true,
				errCode: rpcfb.ErrorCodeNOT_FOUND,
				errMsg:  "stream 1 not found",
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
					{Epoch: 2, Index: 1, Start: 42, End: -1},
				},
			},
		},
		{
			name:    "empty stream",
			prepare: []preRange{},
			args:    args{kind: rpcfb.SealKindPLACEMENT_DRIVER, epoch: 2, index: 0, end: 42},
			want: want{
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_NOT_FOUND,
				errMsg:  "no range in stream 0",
				after:   []*rpcfb.RangeT{},
			},
		},
		{
			name: "range not found",
			prepare: []preRange{
				{end: 42},
				{index: 1, start: 42, end: -1},
			},
			args: args{kind: rpcfb.SealKindPLACEMENT_DRIVER, epoch: 2, index: 2, end: 84},
			want: want{
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_NOT_FOUND,
				errMsg:  "range 2 not found in stream 0",
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
					{Epoch: 2, Index: 1, Start: 42, End: -1},
				},
			},
		},
		{
			name: "range already sealed #1",
			prepare: []preRange{
				{end: 42},
				{index: 1, start: 42, end: -1},
			},
			args: args{kind: rpcfb.SealKindPLACEMENT_DRIVER, epoch: 2, index: 0, end: 84},
			want: want{
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_ALREADY_SEALED,
				errMsg:  "range 0 already sealed in stream 0",
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
					{Epoch: 2, Index: 1, Start: 42, End: -1},
				},
			},
		},
		{
			name: "range already sealed #2",
			prepare: []preRange{
				{end: 42},
				{index: 1, start: 42, end: 84},
			},
			args: args{kind: rpcfb.SealKindPLACEMENT_DRIVER, epoch: 2, index: 1, end: 84},
			want: want{
				wantErr: true,
				errCode: rpcfb.ErrorCodeRANGE_ALREADY_SEALED,
				errMsg:  "range 1 already sealed in stream 0",
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
					{Epoch: 2, Index: 1, Start: 42, End: 84},
				},
			},
		},
		{
			name: "invalid end offset",
			prepare: []preRange{
				{end: 42},
				{index: 1, start: 42, end: -1},
			},
			args: args{kind: rpcfb.SealKindPLACEMENT_DRIVER, epoch: 2, index: 1, end: 21},
			want: want{
				wantErr: true,
				errCode: rpcfb.ErrorCodeBAD_REQUEST,
				errMsg:  "invalid end offset 21 (less than start offset 42) for range 1 in stream 0",
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
					{Epoch: 2, Index: 1, Start: 42, End: -1},
				},
			},
		},
		{
			name: "expired range epoch",
			prepare: []preRange{
				{end: 42},
				{index: 1, start: 42, end: -1},
			},
			args: args{kind: rpcfb.SealKindPLACEMENT_DRIVER, epoch: 1, index: 1, end: 84},
			want: want{
				wantErr: true,
				errCode: rpcfb.ErrorCodeEXPIRED_RANGE_EPOCH,
				errMsg:  "invalid epoch 1 (less than 2) for range 1 in stream 0",
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
					{Epoch: 2, Index: 1, Start: 42, End: -1},
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
			preHeartbeats(t, h, 0, 1, 2)
			streamIDs := preCreateStreams(t, h, 3, 1)
			re.Equal([]int64{0}, streamIDs)
			prepareRanges(t, h, 0, tt.prepare)

			// seal range
			req := &protocol.SealRangeRequest{SealRangeRequestT: rpcfb.SealRangeRequestT{
				Kind:  tt.args.kind,
				Range: &rpcfb.RangeT{Epoch: tt.args.epoch, StreamId: tt.args.streamID, Index: tt.args.index, End: tt.args.end},
			}}
			resp := &protocol.SealRangeResponse{}
			h.SealRange(req, resp)

			// check response
			if tt.want.wantErr {
				re.Equal(tt.want.errCode, resp.Status.Code)
				re.Contains(resp.Status.Message, tt.want.errMsg)
			} else {
				re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code, resp.Status.Message)
				fmtNodes(resp.Range)
				fillRangeInfo(tt.want.returned)
				re.Equal(tt.want.returned, resp.Range)
			}

			// list ranges
			lReq := &protocol.ListRangeRequest{ListRangeRequestT: rpcfb.ListRangeRequestT{
				Criteria: &rpcfb.ListRangeCriteriaT{StreamId: 0, NodeId: -1},
			}}
			lResp := &protocol.ListRangeResponse{}
			h.ListRange(lReq, lResp)
			re.Equal(rpcfb.ErrorCodeOK, lResp.Status.Code, lResp.Status.Message)

			// check list range response
			for _, r := range lResp.Ranges {
				fmtNodes(r)
			}
			for _, r := range tt.want.after {
				fillRangeInfo(r)
			}
			re.Equal(tt.want.after, lResp.Ranges)
		})
	}
}

func TestHandler_CreateRange(t *testing.T) {
	type args struct {
		epoch    int64
		streamID int64
		index    int32
		start    int64
	}
	type want struct {
		returned *rpcfb.RangeT
		wantErr  bool
		errCode  rpcfb.ErrorCode
		errMsg   string
		after    []*rpcfb.RangeT
	}
	tests := []struct {
		name    string
		prepare []preRange
		args    args
		want    want
	}{
		{
			name: "normal case",
			prepare: []preRange{
				{end: 42},
			},
			args: args{epoch: 2, index: 1, start: 42},
			want: want{
				returned: &rpcfb.RangeT{Epoch: 2, Index: 1, Start: 42, End: -1},
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
					{Epoch: 2, Index: 1, Start: 42, End: -1},
				},
			},
		},
		{
			name:    "create the first range",
			prepare: []preRange{},
			args:    args{},
			want: want{
				returned: &rpcfb.RangeT{End: -1},
				after: []*rpcfb.RangeT{
					{End: -1},
				},
			},
		},
		{
			name: "stream not exist",
			prepare: []preRange{
				{end: 42},
			},
			args: args{epoch: 2, streamID: 1, index: 1, start: 42},
			want: want{
				wantErr: true,
				errCode: rpcfb.ErrorCodeNOT_FOUND,
				errMsg:  "stream 1 not found",
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
				},
			},
		},
		{
			name: "create before seal",
			prepare: []preRange{
				{end: 42},
				{index: 1, start: 42, end: -1},
			},
			args: args{epoch: 3, index: 2, start: 84},
			want: want{
				wantErr: true,
				errCode: rpcfb.ErrorCodeCREATE_RANGE_BEFORE_SEAL,
				errMsg:  "create range 2 before sealing the last range 1 in stream 0",
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
					{Epoch: 2, Index: 1, Start: 42, End: -1},
				},
			},
		},
		{
			name: "invalid range index",
			prepare: []preRange{
				{end: 42},
			},
			args: args{epoch: 2, index: 2, start: 42},
			want: want{
				wantErr: true,
				errCode: rpcfb.ErrorCodeBAD_REQUEST,
				errMsg:  "invalid range index 2 (should be 1) in stream 0",
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
				},
			},
		},
		{
			name: "invalid start offset",
			prepare: []preRange{
				{end: 42},
			},
			args: args{epoch: 2, index: 1, start: 84},
			want: want{
				wantErr: true,
				errCode: rpcfb.ErrorCodeBAD_REQUEST,
				errMsg:  "invalid range start 84 (should be 42) for range 1 in stream 0",
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
				},
			},
		},
		{
			name: "expired epoch",
			prepare: []preRange{
				{end: 42},
			},
			args: args{epoch: 0, index: 1, start: 42},
			want: want{
				wantErr: true,
				errCode: rpcfb.ErrorCodeEXPIRED_RANGE_EPOCH,
				errMsg:  "invalid range epoch 0 (less than 1) for range 1 in stream 0",
				after: []*rpcfb.RangeT{
					{Epoch: 1, End: 42},
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
			preHeartbeats(t, h, 0, 1, 2)
			streamIDs := preCreateStreams(t, h, 3, 1)
			re.Equal([]int64{0}, streamIDs)
			prepareRanges(t, h, 0, tt.prepare)

			// create range
			req := &protocol.CreateRangeRequest{CreateRangeRequestT: rpcfb.CreateRangeRequestT{
				Range: &rpcfb.RangeT{Epoch: tt.args.epoch, StreamId: tt.args.streamID, Index: tt.args.index, Start: tt.args.start},
			}}
			resp := &protocol.CreateRangeResponse{}
			h.CreateRange(req, resp)

			// check response
			if tt.want.wantErr {
				re.Equal(tt.want.errCode, resp.Status.Code)
				re.Contains(resp.Status.Message, tt.want.errMsg)
			} else {
				re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code, resp.Status.Message)
				fmtNodes(resp.Range)
				fillRangeInfo(tt.want.returned)
				re.Equal(tt.want.returned, resp.Range)
			}

			// list ranges
			lReq := &protocol.ListRangeRequest{ListRangeRequestT: rpcfb.ListRangeRequestT{
				Criteria: &rpcfb.ListRangeCriteriaT{StreamId: 0, NodeId: -1},
			}}
			lResp := &protocol.ListRangeResponse{}
			h.ListRange(lReq, lResp)
			re.Equal(rpcfb.ErrorCodeOK, lResp.Status.Code, lResp.Status.Message)

			// check list range response
			for _, r := range lResp.Ranges {
				fmtNodes(r)
			}
			for _, r := range tt.want.after {
				fillRangeInfo(r)
			}
			re.Equal(tt.want.after, lResp.Ranges)
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
		Stream: &rpcfb.StreamT{
			Replica:  replica,
			AckCount: replica,
		},
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
			Epoch:    r.Epoch + 1,
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
		Kind: rpcfb.SealKindPLACEMENT_DRIVER,
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

func fmtNodes(r *rpcfb.RangeT) {
	// erase advertise addr
	for _, n := range r.Nodes {
		n.AdvertiseAddr = ""
	}
	// sort by node id
	sort.Slice(r.Nodes, func(i, j int) bool {
		return r.Nodes[i].NodeId < r.Nodes[j].NodeId
	})
}

func fillRangeInfo(r *rpcfb.RangeT) {
	r.Nodes = []*rpcfb.DataNodeT{{NodeId: 0}, {NodeId: 1}, {NodeId: 2}}
	r.ReplicaCount = 3
	r.AckCount = 3
}
