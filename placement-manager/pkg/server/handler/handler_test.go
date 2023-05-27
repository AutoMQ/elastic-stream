package handler

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.uber.org/zap"

	sbpClient "github.com/AutoMQ/placement-manager/pkg/sbp/client"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster"
	"github.com/AutoMQ/placement-manager/pkg/server/config"
	"github.com/AutoMQ/placement-manager/pkg/server/id"
	"github.com/AutoMQ/placement-manager/pkg/server/member"
	"github.com/AutoMQ/placement-manager/pkg/server/storage"
	"github.com/AutoMQ/placement-manager/pkg/server/storage/endpoint"
	"github.com/AutoMQ/placement-manager/pkg/server/storage/kv"
	"github.com/AutoMQ/placement-manager/pkg/util/testutil"
)

type mockServer struct {
	kv        clientv3.KV
	sbpClient sbpClient.Client
}

func (m *mockServer) Storage() storage.Storage {
	etcdKV := kv.NewEtcd(kv.EtcdParam{
		KV:       m.kv,
		RootPath: "/test-server",
		CmpFunc:  func() clientv3.Cmp { return clientv3.Compare(clientv3.CreateRevision("not-exist-key"), "=", 0) },
	}, zap.NewNop())
	return endpoint.NewEndpoint(etcdKV, zap.NewNop())
}

func (m *mockServer) IDAllocator(key string, start, step uint64) id.Allocator {
	return id.NewEtcdAllocator(&id.EtcdAllocatorParam{
		KV:       m.kv,
		CmpFunc:  func() clientv3.Cmp { return clientv3.Compare(clientv3.CreateRevision("not-exist-key"), "=", 0) },
		RootPath: "/test-server",
		Key:      key,
		Start:    start,
		Step:     step,
	}, zap.NewNop())
}

func (m *mockServer) Member() cluster.Member {
	return m
}

func (m *mockServer) IsLeader() bool {
	return true
}

func (m *mockServer) ClusterInfo(_ context.Context) ([]*member.Info, error) {
	return []*member.Info{{
		Name:       "test-member-name",
		MemberID:   1,
		PeerUrls:   []string{"test-member-peer-urls"},
		ClientUrls: []string{"test-member-client-urls"},
		SbpAddr:    "test-member-sbp-addr",
		IsLeader:   true,
	}}, nil
}

func (m *mockServer) SbpClient() sbpClient.Client {
	return m.sbpClient
}

type mockServerNotLeader struct {
	cluster.Server
}

func (m *mockServerNotLeader) IsLeader() bool {
	return false
}

type mockSbpClient struct {
}

func (m mockSbpClient) Do(_ protocol.OutRequest, _ sbpClient.Address) (protocol.InResponse, error) {
	panic("does not mock yet")
}

func startSbpHandler(tb testing.TB, sbpClient sbpClient.Client, isLeader bool) (*Handler, func()) {
	re := require.New(tb)

	if sbpClient == nil {
		sbpClient = mockSbpClient{}
	}

	_, client, closeFunc := testutil.StartEtcd(tb, nil)

	var server cluster.Server
	server = &mockServer{kv: client, sbpClient: sbpClient}
	if !isLeader {
		server = &mockServerNotLeader{server}
	}

	c := cluster.NewRaftCluster(context.Background(), &config.Cluster{SealReqTimeoutMs: 1000, DataNodeTimeout: time.Minute}, server.Member(), zap.NewNop())
	err := c.Start(server)
	re.NoError(err)

	h := NewHandler(c, zap.NewNop())

	return h, func() { _ = c.Stop(); closeFunc() }
}
