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

package cluster

import (
	"context"
	"sync/atomic"

	"go.uber.org/zap"

	sbpClient "github.com/AutoMQ/placement-manager/pkg/sbp/client"
	"github.com/AutoMQ/placement-manager/pkg/server/cluster/cache"
	"github.com/AutoMQ/placement-manager/pkg/server/config"
	"github.com/AutoMQ/placement-manager/pkg/server/id"
	"github.com/AutoMQ/placement-manager/pkg/server/member"
	"github.com/AutoMQ/placement-manager/pkg/server/storage"
	"github.com/AutoMQ/placement-manager/pkg/server/storage/endpoint"
)

const (
	_streamIDAllocKey   = "stream"
	_streamIDStep       = 100
	_dataNodeIDAllocKey = "data-node"
	_dataNodeIDStep     = 1
)

// RaftCluster is used for metadata management.
type RaftCluster struct {
	ctx context.Context

	cfg *config.Cluster

	starting      atomic.Bool
	running       atomic.Bool
	runningCtx    context.Context
	runningCancel context.CancelFunc

	storage storage.Storage
	sAlloc  id.Allocator // stream id allocator
	dnAlloc id.Allocator // data node id allocator
	member  Member
	cache   *cache.Cache
	client  sbpClient.Client

	lg *zap.Logger
}

type Member interface {
	IsLeader() bool
	// ClusterInfo returns all members in the cluster. The first member is the leader.
	ClusterInfo() []*member.Info
}

// Server is the interface for starting a RaftCluster.
type Server interface {
	Storage() storage.Storage
	IDAllocator(key string, start, step uint64) id.Allocator
	Member() Member
	SbpClient() sbpClient.Client
}

// NewRaftCluster creates a new RaftCluster.
func NewRaftCluster(ctx context.Context, cfg *config.Cluster, logger *zap.Logger) *RaftCluster {
	return &RaftCluster{
		ctx:   ctx,
		cfg:   cfg,
		cache: cache.NewCache(),
		lg:    logger,
	}
}

// Start starts the RaftCluster.
func (c *RaftCluster) Start(s Server) error {
	logger := c.lg
	if c.IsRunning() {
		logger.Warn("raft cluster is already running")
		return nil
	}
	if c.starting.Swap(true) {
		logger.Warn("raft cluster is starting")
		return nil
	}
	defer c.starting.Store(false)

	logger.Info("starting raft cluster")

	c.storage = s.Storage()
	c.member = s.Member()
	c.runningCtx, c.runningCancel = context.WithCancel(c.ctx)
	c.sAlloc = s.IDAllocator(_streamIDAllocKey, uint64(endpoint.MinStreamID), _streamIDStep)
	c.dnAlloc = s.IDAllocator(_dataNodeIDAllocKey, uint64(endpoint.MinDataNodeID), _dataNodeIDStep)
	c.client = s.SbpClient()

	c.cache.Reset()

	// start other background goroutines

	if c.running.Swap(true) {
		logger.Warn("raft cluster is already running")
		return nil
	}
	return nil
}

// Stop stops the RaftCluster.
func (c *RaftCluster) Stop() error {
	logger := c.lg
	if !c.running.Swap(false) {
		logger.Warn("raft cluster has already been stopped")
		return nil
	}

	logger.Info("stopping cluster")
	c.runningCancel()

	logger.Info("cluster stopped")
	return nil
}

// IsRunning returns true if the RaftCluster is running.
func (c *RaftCluster) IsRunning() bool {
	return c.running.Load()
}

func (c *RaftCluster) IsLeader() bool {
	return c.member.IsLeader()
}

func (c *RaftCluster) ClusterInfo() []*member.Info {
	return c.member.ClusterInfo()
}
