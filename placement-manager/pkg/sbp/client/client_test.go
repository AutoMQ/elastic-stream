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

	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/sbp/server"
	"github.com/AutoMQ/placement-manager/pkg/server/config"
	tempurl "github.com/AutoMQ/placement-manager/pkg/util/testutil/url"
)

func TestMain(m *testing.M) {
	goleak.VerifyTestMain(m)
}

type timeoutHandler struct {
	baseHandler
	UsedTime time.Duration
}

func (th *timeoutHandler) Heartbeat(_ *protocol.HeartbeatRequest, resp *protocol.HeartbeatResponse) {
	time.Sleep(th.UsedTime)
	resp.OK()
}

type timeoutRequest struct {
	protocol.HeartbeatRequest
	RequestTimeout time.Duration
}

func (tr *timeoutRequest) Timeout() int32 {
	return int32(tr.RequestTimeout / time.Millisecond)
}

func TestClientTimeout(t *testing.T) {
	t.Parallel()
	logger := zap.NewNop()
	re := require.New(t)

	var used = 40 * time.Millisecond
	var timeout = 20 * time.Millisecond

	addr, shutdown := startServer(t, &timeoutHandler{UsedTime: used}, logger)
	defer shutdown()

	client := NewClient(&config.SbpClient{}, logger)
	defer client.Shutdown(context.Background())

	req := &timeoutRequest{RequestTimeout: timeout}
	now := time.Now()
	_, err := client.Do(req, addr)
	cost := time.Since(now)
	re.True(errors.Is(err, context.DeadlineExceeded))
	re.Greater(cost, timeout)
	re.Less(cost, used)
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
