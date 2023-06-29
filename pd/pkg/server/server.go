// Copyright 2016 TiKV Project Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

package server

import (
	"context"
	"net"
	"net/http"
	"path"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/pkg/errors"
	"go.etcd.io/etcd/client/pkg/v3/types"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.etcd.io/etcd/server/v3/embed"
	"go.etcd.io/etcd/server/v3/etcdserver"
	"go.uber.org/zap"
	"go.uber.org/zap/zapcore"
	"google.golang.org/grpc"

	"github.com/AutoMQ/pd/api/kvpb"
	sbpClient "github.com/AutoMQ/pd/pkg/sbp/client"
	sbpServer "github.com/AutoMQ/pd/pkg/sbp/server"
	"github.com/AutoMQ/pd/pkg/server/cluster"
	"github.com/AutoMQ/pd/pkg/server/config"
	"github.com/AutoMQ/pd/pkg/server/handler"
	"github.com/AutoMQ/pd/pkg/server/id"
	"github.com/AutoMQ/pd/pkg/server/member"
	"github.com/AutoMQ/pd/pkg/server/storage"
	"github.com/AutoMQ/pd/pkg/server/storage/endpoint"
	"github.com/AutoMQ/pd/pkg/server/storage/kv"
	"github.com/AutoMQ/pd/pkg/util/etcdutil"
	"github.com/AutoMQ/pd/pkg/util/logutil"
	"github.com/AutoMQ/pd/pkg/util/randutil"
	"github.com/AutoMQ/pd/pkg/util/typeutil"
)

const (
	_etcdTimeout              = time.Second * 3       // etcd DialTimeout
	_etcdStartTimeout         = time.Minute * 5       // timeout when start etcd
	_leaderTickInterval       = 50 * time.Millisecond // check leader loop interval
	_shutdownSbpServerTimeout = time.Second * 5       // timeout when shutdown sbp server

	_rootPathPrefix = "/pd"                          // prefix of Server.rootPath
	_clusterIDPath  = "/placement-driver/cluster_id" // path of Server.clusterID
)

// Server ensures redundancy by using the Raft consensus algorithm provided by etcd
type Server struct {
	started atomic.Bool // server status, true for started

	cfg *config.Config // Server configuration

	ctx        context.Context    // main context
	loopCtx    context.Context    // loop context
	loopCancel context.CancelFunc // loop cancel
	loopWg     sync.WaitGroup     // loop wait group

	member    *member.Member   // for leader election
	client    *clientv3.Client // etcd client
	clusterID uint64           // PD cluster id
	rootPath  string           // root path in etcd

	storage   storage.Storage
	cluster   *cluster.RaftCluster
	sbpServer *sbpServer.Server
	sbpClient sbpClient.Client

	lg *zap.Logger // logger
}

// NewServer creates the UNINITIALIZED PD server with given configuration.
func NewServer(ctx context.Context, cfg *config.Config, logger *zap.Logger) (*Server, error) {
	s := &Server{
		cfg:    cfg,
		ctx:    ctx,
		member: &member.Member{},
		lg:     logger,
	}

	s.cfg.Etcd.ServiceRegister = func(gs *grpc.Server) {
		kvpb.RegisterKVServer(gs, NewGrpcServer(s, logger))
	}

	return s, nil
}

// Start starts the server
func (s *Server) Start() error {
	if err := s.startEtcd(s.ctx); err != nil {
		return errors.Wrap(err, "start etcd")
	}
	if err := s.startServer(); err != nil {
		return errors.Wrap(err, "start server")
	}
	s.startLoop(s.ctx)

	return nil
}

func (s *Server) startEtcd(ctx context.Context) error {
	startTimeoutCtx, cancel := context.WithTimeout(ctx, _etcdStartTimeout)
	defer cancel()

	logger := s.lg

	etcd, err := embed.StartEtcd(s.cfg.Etcd)
	if err != nil && strings.Contains(err.Error(), "has already been bootstrapped") {
		logger.Warn("member has been bootstrapped, set ClusterState = \"existing\" and try again")
		s.cfg.Etcd.ClusterState = embed.ClusterStateFlagExisting
		etcd, err = embed.StartEtcd(s.cfg.Etcd)
	}
	if err != nil {
		return errors.Wrap(err, "start etcd by config")
	}

	// Check cluster ID
	urlMap, err := types.NewURLsMap(s.cfg.InitialCluster)
	if err != nil {
		logger.Error("failed to parse urls map from config", zap.String("config-initial-cluster", s.cfg.InitialCluster), zap.Error(err))
		return errors.Wrap(err, "parse urlMap from config")
	}
	err = checkClusterID(etcd.Server.Cluster().ID(), urlMap, logger)
	if err != nil {
		return errors.Wrap(err, "check cluster ID")
	}

	// wait until etcd is ready or timeout
	select {
	case <-etcd.Server.ReadyNotify():
	case <-startTimeoutCtx.Done():
		return errors.New("failed to start etcd: timeout")
	}
	logger.Info("etcd started")

	// init client
	endpoints := make([]string, 0, len(s.cfg.Etcd.ACUrls))
	for _, url := range s.cfg.Etcd.ACUrls {
		endpoints = append(endpoints, url.String())
	}
	etcdLogLevel, _ := zapcore.ParseLevel(s.cfg.Etcd.LogLevel)
	etcdLogger := logger
	if logger.Core().Enabled(etcdLogLevel) {
		etcdLogger = logger.WithOptions(zap.IncreaseLevel(etcdLogLevel))
	}
	client, err := clientv3.New(clientv3.Config{
		Endpoints:   endpoints,
		DialTimeout: _etcdTimeout,
		Logger:      etcdLogger.With(zap.Namespace("etcd-client"), zap.Strings("endpoints", endpoints)),
	})

	if err != nil {
		return errors.Wrap(err, "new client")
	}
	logger.Info("new etcd client", zap.Strings("endpoints", endpoints))
	s.client = client

	// init member
	s.member = member.NewMember(etcd, client, logger)

	return nil
}

func (s *Server) startServer() error {
	// init cluster id
	if err := s.initClusterID(); err != nil {
		return errors.Wrap(err, "init cluster ID")
	}

	logger := s.lg.With(zap.Uint64("cluster-id", s.clusterID))
	logger.Info("init cluster ID")

	s.rootPath = path.Join(_rootPathPrefix, strconv.FormatUint(s.clusterID, 10))
	err := s.member.Init(s.ctx, s.cfg, s.Name(), s.rootPath)
	if err != nil {
		return errors.Wrap(err, "init member")
	}
	etcdKV := kv.Logger{KV: kv.NewEtcd(kv.EtcdParam{
		KV:        s.client,
		RootPath:  s.rootPath,
		CmpFunc:   s.leaderCmp,
		MaxTxnOps: s.cfg.Etcd.MaxTxnOps,
	}, logger)}
	s.storage = endpoint.NewEndpoint(etcdKV, logger)
	s.cluster = cluster.NewRaftCluster(s.ctx, s.cfg.Cluster, s.member, logger)
	s.sbpClient = sbpClient.NewClient(s.cfg.Sbp.Client, logger)

	pdAddr := s.cfg.PDAddr
	listener, err := net.Listen("tcp", pdAddr)
	if err != nil {
		return errors.Wrapf(err, "listen on %s", pdAddr)
	}
	go s.serveSbp(listener, s.cluster)

	if s.started.Swap(true) {
		logger.Warn("server already started")
	}
	return nil
}

func (s *Server) serveSbp(listener net.Listener, c *cluster.RaftCluster) {
	logger := s.lg.With(zap.String("listener-addr", listener.Addr().String()))

	ctx, cancel := context.WithCancel(s.ctx)
	defer cancel()

	sbpSvr := sbpServer.NewServer(ctx, s.cfg.Sbp.Server, handler.Checker{Handler: handler.NewHandler(c, logger)}, logger)
	s.sbpServer = sbpSvr

	logger.Info("sbp server started")
	if err := sbpSvr.Serve(listener); err != nil && err != sbpServer.ErrServerClosed {
		logger.Error("sbp server failed", zap.Error(err))
	}
}

func (s *Server) initClusterID() error {
	logger := s.lg

	// query any existing ID in etcd
	keyValue, err := etcdutil.GetOne(s.ctx, s.client, []byte(_clusterIDPath), logger)
	if err != nil {
		logger.Error("failed to query cluster id", zap.String("cluster-id-path", _clusterIDPath), zap.Error(err))
		return errors.Wrap(err, "get value from etcd")
	}

	// use an existed ID
	if keyValue != nil {
		s.clusterID, err = typeutil.BytesToUint64(keyValue.Value)
		logger.Info("use an existing cluster id", zap.Uint64("cluster-id", s.clusterID))
		return errors.Wrap(err, "convert bytes to uint64")
	}

	// new an ID
	s.clusterID, err = initOrGetClusterID(s.client, _clusterIDPath)
	if err != nil {
		return errors.Wrap(err, "new an ID")
	}
	logger.Info("use a new cluster id", zap.Uint64("cluster-id", s.clusterID))
	return nil
}

func (s *Server) startLoop(ctx context.Context) {
	s.loopCtx, s.loopCancel = context.WithCancel(ctx)
	loops := []func(){s.leaderLoop, s.etcdLeaderLoop}

	s.loopWg.Add(len(loops))
	for _, loop := range loops {
		go loop()
	}
}

func (s *Server) leaderLoop() {
	logger := s.lg
	defer logutil.LogPanicAndExit(logger)
	defer s.loopWg.Done()

	for {
		if s.IsClosed() {
			logger.Info("server is closed. stop leader loop")
			return
		}

		leader, rev, checkAgain := s.member.CheckLeader(s.ctx)
		if checkAgain {
			continue
		}

		if leader != nil {
			logger.Info("start to watch PD leader", zap.Object("pd-leader", leader))
			// WatchLeader will keep looping and never return unless the PD leader has changed.
			s.member.WatchLeader(s.loopCtx, leader, rev)
			logger.Info("PD leader has changed, try to re-campaign a PD leader")
		}

		// To make sure the etcd leader and PD leader are on the same server.
		if leaderID := s.member.EtcdLeaderID(); leaderID != s.member.ID() {
			logger.Info("etcd leader and PD leader are not on the same server, skip campaigning of PD leader and check later",
				zap.String("server-name", s.Name()), zap.Uint64("etcd-leader-id", leaderID), zap.Uint64("member-id", s.member.ID()))
			time.Sleep(member.CheckAgainInterval)
			continue
		}
		s.campaignLeader()
	}
}

func (s *Server) campaignLeader() {
	logger := s.lg
	logger.Info("start to campaign PD leader", zap.String("campaign-pd-leader-name", s.Name()))

	// campaign leader
	success, err := s.member.CampaignLeader(s.ctx, s.cfg.LeaderLease)
	if err != nil {
		logger.Error("an error when campaign leader", zap.String("campaign-pd-leader-name", s.Name()), zap.Error(err))
		return
	}
	if !success {
		logger.Info("failed to campaign leader", zap.String("campaign-pd-leader-name", s.Name()))
		return
	}

	// Start keepalive the leadership
	ctx, cancel := context.WithCancel(s.loopCtx)
	var resetLeaderOnce sync.Once
	resetLeaderFunc := func() {
		cancel()
		s.member.ResetLeader()
	}
	defer resetLeaderOnce.Do(resetLeaderFunc)
	// maintain the PD leadership
	s.member.KeepLeader(ctx)
	logger.Info("success to campaign leader", zap.String("campaign-pd-leader-name", s.Name()))

	err = s.cluster.Start(s)
	if err != nil {
		logger.Error("failed to start cluster", zap.Error(err))
		return
	}
	defer func() { _ = s.cluster.Stop() }()

	// EnableLeader to accept requests
	s.member.EnableLeader()
	// as soon as cancel the leadership keepalive, then other member have chance to be new leader.
	defer resetLeaderOnce.Do(resetLeaderFunc)
	logger.Info("PD cluster leader is ready to serve", zap.String("pd-leader-name", s.Name()))

	s.checkLeaderLoop(ctx)
}

func (s *Server) checkLeaderLoop(ctx context.Context) {
	logger := s.lg

	leaderTicker := time.NewTicker(_leaderTickInterval)
	defer leaderTicker.Stop()

	for {
		select {
		case <-leaderTicker.C:
			if !s.member.IsLeader() {
				logger.Info("no longer a leader because lease has expired, pd leader will step down")
				return
			}
			if etcdLeader := s.member.EtcdLeaderID(); etcdLeader != s.member.ID() {
				logger.Info("etcd leader changed, resigns pd leadership", zap.String("old-pd-leader-name", s.Name()))
				return
			}
		case <-ctx.Done():
			// Server is closed and it should return nil.
			logger.Info("server is closed")
			return
		}
	}
}

func (s *Server) etcdLeaderLoop() {
	logger := s.lg
	defer logutil.LogPanicAndExit(logger)
	defer s.loopWg.Done()

	ctx, cancel := context.WithCancel(s.loopCtx)
	defer cancel()
	for {
		select {
		case <-time.After(s.cfg.LeaderPriorityCheckInterval):
			err := s.member.CheckPriorityAndMoveLeader(ctx)
			if err != nil {
				logger.Error("failed to check priority and move leader", zap.Error(err))
			}
		case <-ctx.Done():
			logger.Info("server is closed, stop etcd leader loop")
			return
		}
	}
}

// Name returns the unique etcd Name for this server in etcd cluster.
func (s *Server) Name() string {
	return s.cfg.Name
}

// Context returns the context of server.
func (s *Server) Context() context.Context {
	return s.ctx
}

func (s *Server) Storage() storage.Storage {
	return s.storage
}

func (s *Server) Member() cluster.Member {
	return s.member
}

func (s *Server) SbpClient() sbpClient.Client {
	return s.sbpClient
}

func (s *Server) IDAllocator(key string, start, step uint64) id.Allocator {
	return id.Logger{Allocator: id.NewEtcdAllocator(&id.EtcdAllocatorParam{
		KV:       s.client,
		CmpFunc:  s.leaderCmp,
		RootPath: s.rootPath,
		Key:      key,
		Start:    start,
		Step:     step,
	}, s.lg)}
}

// IsClosed checks whether server is closed or not.
func (s *Server) IsClosed() bool {
	return !s.started.Load()
}

// Close closes the server.
func (s *Server) Close() {
	if !s.started.Swap(false) {
		// server is already closed
		return
	}

	logger := s.lg
	logger.Info("closing server")

	s.stopServerLoop()
	s.stopSbpServer()

	if s.client != nil {
		if err := s.client.Close(); err != nil {
			logger.Error("failed to close etcd client", zap.Error(err))
		}
	}

	if s.member.Etcd() != nil {
		s.member.Etcd().Close()
	}

	logger.Info("server closed")
}

func (s *Server) stopServerLoop() {
	s.loopCancel()
	s.loopWg.Wait()
}

func (s *Server) stopSbpServer() {
	ctx, cancel := context.WithTimeout(context.Background(), _shutdownSbpServerTimeout)
	defer cancel()
	_ = s.sbpServer.Shutdown(ctx)
}

// leaderCmp returns a cmp with leader comparison to guarantee that
// the transaction can be executed only if the server is leader.
func (s *Server) leaderCmp() clientv3.Cmp {
	return clientv3.Compare(clientv3.Value(s.member.LeaderPath()), "=", string(s.member.Info()))
}

// checkClusterID checks etcd cluster ID, returns an error if mismatched.
// This function will never block even quorum is not satisfied.
func checkClusterID(localClusterID types.ID, um types.URLsMap, logger *zap.Logger) error {
	if len(um) == 0 {
		return nil
	}

	for _, u := range um.URLs() {
		trp := &http.Transport{}
		remoteCluster, err := etcdserver.GetClusterFromRemotePeers(nil, []string{u}, trp)
		trp.CloseIdleConnections()
		if err != nil {
			// Do not return error, because other members may be not ready.
			logger.Warn("failed to get cluster from remote", zap.Error(err))
			continue
		}

		if remoteClusterID := remoteCluster.ID(); remoteClusterID != localClusterID {
			logger.Error("invalid cluster id", zap.Uint64("expected", uint64(localClusterID)), zap.Uint64("got", uint64(remoteClusterID)))
			return errors.Errorf("Etcd cluster ID mismatch, expected %d, got %d", localClusterID, remoteClusterID)
		}
	}
	return nil
}

func initOrGetClusterID(c *clientv3.Client, key string) (uint64, error) {
	ctx, cancel := context.WithTimeout(c.Ctx(), etcdutil.DefaultRequestTimeout)
	defer cancel()

	// Generate a random cluster ID.
	ts := uint64(time.Now().Unix())
	rd, err := randutil.Uint64()
	if err != nil {
		return 0, errors.Wrap(err, "generate random int64")
	}
	ID := (ts << 32) + rd
	value := typeutil.Uint64ToBytes(ID)

	// Multiple PD nodes may try to init the cluster ID at the same time.
	// Only one PD can commit this transaction, then other PD nodes can get
	// the committed cluster ID.
	resp, err := c.Txn(ctx).
		If(clientv3.Compare(clientv3.CreateRevision(key), "=", 0)).
		Then(clientv3.OpPut(key, string(value))).
		Else(clientv3.OpGet(key)).
		Commit()
	if err != nil {
		return 0, errors.Wrap(err, "init cluster ID by etcd transaction")
	}

	// Txn commits ok, return the generated cluster ID.
	if resp.Succeeded {
		return ID, nil
	}

	// Otherwise, parse the committed cluster ID.
	if len(resp.Responses) == 0 {
		return 0, errors.New("etcd transaction failed, conflicted and rolled back")
	}
	response := resp.Responses[0].GetResponseRange()
	if response == nil || len(response.Kvs) != 1 {
		return 0, errors.New("etcd transaction failed, conflicted and rolled back")
	}
	return typeutil.BytesToUint64(response.Kvs[0].Value)
}
