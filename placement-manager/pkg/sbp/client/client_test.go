package client

import (
	"context"
	"net"
	"testing"
	"time"

	"github.com/pkg/errors"
	"github.com/stretchr/testify/require"
	"go.uber.org/goleak"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/sbp/server"
	"github.com/AutoMQ/placement-manager/pkg/server/config"
	tempurl "github.com/AutoMQ/placement-manager/pkg/util/testutil/url"
)

func TestMain(m *testing.M) {
	goleak.VerifyTestMain(m)
}

func TestClient_SealRanges(t *testing.T) {
	t.Parallel()
	logger := zap.NewNop()
	re := require.New(t)

	addr, shutdown := startServer(t, &mockHandler{}, logger)
	defer shutdown()

	client := NewClient(&config.SbpClient{}, logger)
	defer client.Shutdown(context.Background())

	req := &protocol.SealRangesRequest{SealRangesRequestT: rpcfb.SealRangesRequestT{
		Ranges: []*rpcfb.RangeIdT{
			{
				StreamId:   1,
				RangeIndex: 2,
			},
		},
	}}
	resp, err := client.SealRanges(req, addr)
	re.NoError(err)
	re.Equal(rpcfb.ErrorCodeOK, resp.Status.Code)
	re.Len(resp.SealResponses, 1)
	re.Equal(rpcfb.ErrorCodeOK, resp.SealResponses[0].Status.Code)
	re.Equal(int32(2), resp.SealResponses[0].Range.RangeIndex)
}

func TestClientTimeout(t *testing.T) {
	t.Parallel()
	logger := zap.NewNop()
	re := require.New(t)

	addr, shutdown := startServer(t, &mockHandler{SealRangesF: func(req *protocol.SealRangesRequest, resp *protocol.SealRangesResponse) {
		time.Sleep(40 * time.Millisecond)
	}}, logger)
	defer shutdown()

	client := NewClient(&config.SbpClient{}, logger)
	defer client.Shutdown(context.Background())

	req := &protocol.SealRangesRequest{SealRangesRequestT: rpcfb.SealRangesRequestT{
		TimeoutMs: 20,
	}}
	now := time.Now()
	_, err := client.SealRanges(req, addr)
	cost := time.Since(now)
	re.True(errors.Is(err, context.DeadlineExceeded))
	re.Greater(cost, 20*time.Millisecond)
	re.Less(cost, 40*time.Millisecond)
}

func startServer(tb testing.TB, handler server.Handler, lg *zap.Logger) (addr string, shutdown func()) {
	re := require.New(tb)

	addr = tempurl.AllocAddr(tb)
	listener, err := net.Listen("tcp", addr)
	re.NoError(err)

	s := server.NewServer(context.Background(), &config.SbpServer{}, handler, lg)
	go func() {
		_ = s.Serve(listener)
	}()

	shutdown = func() {
		_ = s.Shutdown(context.Background())
	}
	return
}

type mockHandler struct {
	baseHandler

	SealRangesF func(req *protocol.SealRangesRequest, resp *protocol.SealRangesResponse)
}

func normalSealRanges(req *protocol.SealRangesRequest, resp *protocol.SealRangesResponse) {
	results := make([]*rpcfb.SealRangesResultT, 0, len(req.Ranges))
	for _, r := range req.Ranges {
		results = append(results, &rpcfb.SealRangesResultT{
			Range: &rpcfb.RangeT{
				StreamId:    r.StreamId,
				RangeIndex:  r.RangeIndex,
				StartOffset: 1024,
				EndOffset:   2048,
			},
			Status: &rpcfb.StatusT{Code: rpcfb.ErrorCodeOK},
		})
	}
	resp.SealRangesResponseT = rpcfb.SealRangesResponseT{
		SealResponses: results,
	}
	resp.OK()
}

func (m *mockHandler) SealRanges(req *protocol.SealRangesRequest, resp *protocol.SealRangesResponse) {
	if m.SealRangesF != nil {
		m.SealRangesF(req, resp)
		return
	}
	normalSealRanges(req, resp)
}

type baseHandler struct{}

func (b *baseHandler) Heartbeat(_ *protocol.HeartbeatRequest, resp *protocol.HeartbeatResponse) {
	resp.OK()
}

func (b *baseHandler) AllocateID(_ *protocol.IDAllocationRequest, resp *protocol.IDAllocationResponse) {
	resp.OK()
}

func (b *baseHandler) SealRanges(_ *protocol.SealRangesRequest, resp *protocol.SealRangesResponse) {
	resp.OK()
}

func (b *baseHandler) ListRanges(_ *protocol.ListRangesRequest, resp *protocol.ListRangesResponse) {
	resp.OK()
}

func (b *baseHandler) CreateStreams(_ *protocol.CreateStreamsRequest, resp *protocol.CreateStreamsResponse) {
	resp.OK()
}

func (b *baseHandler) DeleteStreams(_ *protocol.DeleteStreamsRequest, resp *protocol.DeleteStreamsResponse) {
	resp.OK()
}

func (b *baseHandler) UpdateStreams(_ *protocol.UpdateStreamsRequest, resp *protocol.UpdateStreamsResponse) {
	resp.OK()
}

func (b *baseHandler) DescribeStreams(_ *protocol.DescribeStreamsRequest, resp *protocol.DescribeStreamsResponse) {
	resp.OK()
}

func (b *baseHandler) DescribePMCluster(_ *protocol.DescribePMClusterRequest, resp *protocol.DescribePMClusterResponse) {
	resp.OK()
}
