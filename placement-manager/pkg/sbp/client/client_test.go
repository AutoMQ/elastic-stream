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

func (b *baseHandler) SealRange(_ *protocol.SealRangeRequest, resp *protocol.SealRangeResponse) {
	resp.OK()
}

func (b *baseHandler) ListRange(_ *protocol.ListRangeRequest, resp *protocol.ListRangeResponse) {
	resp.OK()
}

func (b *baseHandler) CreateStream(_ *protocol.CreateStreamRequest, resp *protocol.CreateStreamResponse) {
	resp.OK()
}

func (b *baseHandler) DeleteStream(_ *protocol.DeleteStreamRequest, resp *protocol.DeleteStreamResponse) {
	resp.OK()
}

func (b *baseHandler) UpdateStream(_ *protocol.UpdateStreamRequest, resp *protocol.UpdateStreamResponse) {
	resp.OK()
}

func (b *baseHandler) DescribeStream(_ *protocol.DescribeStreamRequest, resp *protocol.DescribeStreamResponse) {
	resp.OK()
}

func (b *baseHandler) DescribePMCluster(_ *protocol.DescribePMClusterRequest, resp *protocol.DescribePMClusterResponse) {
	resp.OK()
}
