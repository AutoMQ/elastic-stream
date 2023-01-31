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
	"math/rand"
	"path"
	"strconv"
	"sync"
	"sync/atomic"
	"time"

	"github.com/pkg/errors"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.etcd.io/etcd/server/v3/embed"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/pkg/server/config"
	"github.com/AutoMQ/placement-manager/pkg/server/member"
	"github.com/AutoMQ/placement-manager/pkg/util/etcdutil"
	"github.com/AutoMQ/placement-manager/pkg/util/typeutil"
)

const (
	_etcdTimeout      = time.Second * 3 // etcd DialTimeout
	_etcdStartTimeout = time.Minute * 5 // timeout when start etcd

	_rootPathPrefix = "/placement-manager"           // prefix of Server.rootPath
	_serverIDPath   = "/placement-manager/server_id" // path of Server.id
)

// Server ensures redundancy by using the Raft consensus algorithm provided by etcd
type Server struct {
	started atomic.Bool // server status, true for started

	cfg     *config.Config // Server configuration
	etcdCfg *embed.Config  // etcd configuration

	ctx        context.Context // main context
	loopCtx    context.Context // loop context
	loopCancel func()          // loop cancel
	loopWg     sync.WaitGroup  // loop wait group

	member   *member.Member   // for leader election
	client   *clientv3.Client // etcd client
	id       uint64           // server id
	rootPath string           // root path in etcd

	lg *zap.Logger // logger
}

// NewServer creates the UNINITIALIZED pd server with given configuration.
func NewServer(ctx context.Context, cfg *config.Config) (*Server, error) {
	rand.Seed(time.Now().UnixNano())

	s := &Server{
		cfg:    cfg,
		ctx:    ctx,
		member: &member.Member{},
	}
	s.started.Store(false)

	// etcd Config
	etcdCfg, err := s.cfg.GenEmbedEtcdConfig()
	if err != nil {
		return nil, errors.Wrap(err, "generate etcd config")
	}
	s.etcdCfg = etcdCfg

	return s, nil
}

// Start starts the server
func (s *Server) Start() error {
	if err := s.startEtcd(s.ctx); err != nil {
		return errors.Wrap(err, "start etcd")
	}
	if err := s.startServer(s.ctx); err != nil {
		return errors.Wrap(err, "start server")
	}
	s.startLoop(s.ctx)

	return nil
}

func (s *Server) startEtcd(ctx context.Context) error {
	startTimeoutCtx, cancel := context.WithTimeout(ctx, _etcdStartTimeout)
	defer cancel()

	etcd, err := embed.StartEtcd(s.etcdCfg)
	if err != nil {
		return errors.Wrap(err, "start etcd by config")
	}

	// wait until etcd is ready or timeout
	select {
	case <-etcd.Server.ReadyNotify():
	case <-startTimeoutCtx.Done():
		return errors.New("failed to start etcd: timeout")
	}

	// init client
	endpoints := make([]string, 0, len(s.etcdCfg.ACUrls))
	for _, url := range s.etcdCfg.ACUrls {
		endpoints = append(endpoints, url.String())
	}
	client, err := clientv3.New(clientv3.Config{
		Endpoints:   endpoints,
		DialTimeout: _etcdTimeout,
		Logger:      s.lg,
	})
	if err != nil {
		return errors.Wrap(err, "new client")
	}
	s.client = client

	// init member
	s.member = member.NewMember(etcd, client, uint64(etcd.Server.ID()))

	return nil
}

func (s *Server) startServer(ctx context.Context) error {
	// init server id
	if err := s.initID(); err != nil {
		return errors.Wrap(err, "init server ID")
	}

	s.rootPath = path.Join(_rootPathPrefix, strconv.FormatUint(s.id, 10))
	s.member.Init(s.cfg, s.Name(), s.rootPath)
	// TODO set member prop

	if s.started.Swap(true) {
		s.lg.Warn("server already started.")
	}
	return nil
}

func (s *Server) initID() error {
	// query any existing ID in etcd
	resp, err := etcdutil.GetValue(s.client, _serverIDPath)
	if err != nil {
		return errors.Wrap(err, "get value from etcd")
	}

	// use an existed ID
	if len(resp.Kvs) != 0 {
		s.id, err = typeutil.BytesToUint64(resp.Kvs[0].Value)
		return errors.Wrap(err, "convert bytes to uint64")
	}

	// new an ID
	s.id, err = initOrGetServerID(s.client, _serverIDPath)
	return errors.Wrap(err, "new an ID")
}
func (s *Server) startLoop(ctx context.Context) {
	// TODO
}

// Close closes the server.
func (s *Server) Close() {
	if !s.started.Swap(false) {
		// server is already closed
		return
	}
	// TODO stop loop, close etcd, etc.
}

// IsClosed checks whether server is closed or not.
func (s *Server) IsClosed() bool {
	return !s.started.Load()
}

func (s *Server) Name() string {
	// TODO
	return ""
}

func initOrGetServerID(c *clientv3.Client, key string) (uint64, error) {
	ctx, cancel := context.WithTimeout(c.Ctx(), etcdutil.DefaultRequestTimeout)
	defer cancel()

	// Generate a random server ID.
	ts := uint64(time.Now().Unix())
	ID := (ts << 32) + uint64(rand.Uint32())
	value := typeutil.Uint64ToBytes(ID)

	// Multiple PDs may try to init the server ID at the same time.
	// Only one PD can commit this transaction, then other PDs can get
	// the committed server ID.
	resp, err := c.Txn(ctx).
		If(clientv3.Compare(clientv3.CreateRevision(key), "=", 0)).
		Then(clientv3.OpPut(key, string(value))).
		Else(clientv3.OpGet(key)).
		Commit()
	if err != nil {
		return 0, errors.Wrap(err, "init server ID by etcd transaction")
	}

	// Txn commits ok, return the generated server ID.
	if resp.Succeeded {
		return ID, nil
	}

	// Otherwise, parse the committed server ID.
	if len(resp.Responses) == 0 {
		return 0, errors.New("etcd transaction failed, conflicted and rolled back")
	}
	response := resp.Responses[0].GetResponseRange()
	if response == nil || len(response.Kvs) != 1 {
		return 0, errors.New("etcd transaction failed, conflicted and rolled back")
	}
	return typeutil.BytesToUint64(response.Kvs[0].Value)
}
