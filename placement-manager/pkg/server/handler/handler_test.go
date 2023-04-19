package handler

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/api/rpcfb/rpcfb"
	sbpClient "github.com/AutoMQ/placement-manager/pkg/sbp/client"
	"github.com/AutoMQ/placement-manager/pkg/sbp/protocol"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster"
	"github.com/AutoMQ/placement-manager/pkg/server/config"
	"github.com/AutoMQ/placement-manager/pkg/server/id"
	"github.com/AutoMQ/placement-manager/pkg/server/member"
	"github.com/AutoMQ/placement-manager/pkg/server/storage"
	"github.com/AutoMQ/placement-manager/pkg/util/randutil"
	"github.com/AutoMQ/placement-manager/pkg/util/testutil"
)

type mockServer struct {
	c         *clientv3.Client
	sbpClient sbpClient.Client
}

func (m *mockServer) Storage() storage.Storage {
	return storage.NewEtcd(
		m.c,
		"/test-server",
		zap.NewNop(),
		func() clientv3.Cmp { return clientv3.Compare(clientv3.CreateRevision("not-exist-key"), "=", 0) },
	)
}

func (m *mockServer) IDAllocator(key string, start, step uint64) id.Allocator {
	return id.NewEtcdAllocator(&id.EtcdAllocatorParam{
		Client:   m.c,
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

func (m *mockServer) ClusterInfo() []*member.Info {
	return []*member.Info{{
		Name:       "test-member-name",
		MemberID:   1,
		PeerUrls:   []string{"test-member-peer-urls"},
		ClientUrls: []string{"test-member-client-urls"},
		SbpAddr:    "test-member-sbp-addr",
	}}
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
	endOffsetF func(rangeIndex int32, addr sbpClient.Address) int64
}

func (m mockSbpClient) Do(_ protocol.OutRequest, _ sbpClient.Address) (protocol.InResponse, error) {
	panic("does not mock yet")
}

func (m mockSbpClient) SealRanges(req *protocol.SealRangesRequest, addr sbpClient.Address) (*protocol.SealRangesResponse, error) {
	results := make([]*rpcfb.SealRangesResultT, 0, len(req.Ranges))
	for _, rangeID := range req.Ranges {
		start := int64(rangeID.RangeIndex * 100)
		end := start
		if m.endOffsetF != nil {
			end = m.endOffsetF(rangeID.RangeIndex, addr)
		} else {
			n, _ := randutil.Uint64()
			end += int64(n % 100)
		}

		if end == -1 {
			results = append(results, &rpcfb.SealRangesResultT{
				Status: &rpcfb.StatusT{
					Code: rpcfb.ErrorCodeDN_NOT_IMPLEMENTED,
				},
			})
			continue
		}

		results = append(results, &rpcfb.SealRangesResultT{
			Range: &rpcfb.RangeT{
				StreamId:    rangeID.StreamId,
				RangeIndex:  rangeID.RangeIndex,
				StartOffset: start,
				EndOffset:   end,
			},
			Status: &rpcfb.StatusT{
				Code: rpcfb.ErrorCodeOK,
			},
		})
	}

	resp := &protocol.SealRangesResponse{SealRangesResponseT: rpcfb.SealRangesResponseT{
		SealResponses: results,
	}}
	resp.OK()
	return resp, nil
}

func startSbpHandler(tb testing.TB, sbpClient sbpClient.Client, isLeader bool) (*Handler, func()) {
	re := require.New(tb)

	if sbpClient == nil {
		sbpClient = mockSbpClient{}
	}

	_, client, closeFunc := testutil.StartEtcd(tb)

	var server cluster.Server
	server = &mockServer{c: client, sbpClient: sbpClient}
	if !isLeader {
		server = &mockServerNotLeader{server}
	}

	c := cluster.NewRaftCluster(context.Background(), &config.Cluster{SealReqTimeoutMs: 1000, DataNodeTimeout: time.Minute}, zap.NewNop())
	err := c.Start(server)
	re.NoError(err)

	h := NewHandler(c, zap.NewNop())

	return h, func() { _ = c.Stop(); closeFunc() }
}
