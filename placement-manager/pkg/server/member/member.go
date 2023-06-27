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

package member

import (
	"context"
	"encoding/json"
	"path"
	"strconv"
	"strings"
	"sync/atomic"
	"time"

	"github.com/pkg/errors"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.etcd.io/etcd/server/v3/embed"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/pkg/server/config"
	"github.com/AutoMQ/placement-manager/pkg/server/election"
	"github.com/AutoMQ/placement-manager/pkg/util/etcdutil"
	"github.com/AutoMQ/placement-manager/pkg/util/traceutil"
)

const (
	// CheckAgainInterval is interval at which the CheckLeader method is rechecked if an unexpected event occurs
	CheckAgainInterval = 200 * time.Millisecond

	_memberPathPrefix = "member"

	_leaderPathPrefix   = "leader"
	_priorityPathPrefix = "priority"
	_infoPathPrefix     = "info"

	_leaderElectionPurpose = "PM leader election"

	_moveLeaderTimeout = 5 * time.Second // The timeout to wait transfer etcd leader to complete
)

// Member is used for the election related logic.
type Member struct {
	leadership *election.Leadership
	leader     atomic.Pointer[Info] // current leader's Info

	etcd            *embed.Etcd
	client          *clientv3.Client
	id              uint64 // etcd server id.
	clusterRootPath string // cluster root path in etcd

	// info is current PM's Info.
	// It will be serialized (infoValue) and saved in etcd leader key when the PM node
	// is successfully elected as the PM leader of the cluster.
	// Every write will use it to check PM leadership.
	info      *Info
	infoValue []byte

	lg *zap.Logger // logger
}

// NewMember create a new Member.
func NewMember(etcd *embed.Etcd, client *clientv3.Client, logger *zap.Logger) *Member {
	id := uint64(etcd.Server.ID())
	return &Member{
		etcd:   etcd,
		client: client,
		id:     id,
		lg:     logger.With(zap.String("member-id", strconv.FormatUint(id, 10))),
	}
}

// Init initializes the member info.
func (m *Member) Init(ctx context.Context, cfg *config.Config, name string, clusterRootPath string) error {
	info := &Info{
		Name:            name,
		MemberID:        m.id,
		ClientUrls:      strings.Split(cfg.AdvertiseClientUrls, config.URLSeparator),
		PeerUrls:        strings.Split(cfg.AdvertisePeerUrls, config.URLSeparator),
		AdvertisePMAddr: cfg.AdvertisePMAddr,
	}
	logger := m.lg.With(zap.Object("member-info", info))

	logger.Info("start to init member info")

	m.info = info
	bytes, err := json.Marshal(info)
	if err != nil {
		logger.Error("failed to marshal member info", zap.Error(err))
		return errors.Wrap(err, "marshal member info")
	}
	m.infoValue = bytes
	m.clusterRootPath = clusterRootPath
	m.leadership = election.NewLeadership(m.client, m.LeaderPath(), _leaderElectionPurpose, m.lg)

	err = m.saveInfo(ctx, m.infoValue)
	if err != nil {
		logger.Error("failed to save member info", zap.Error(err))
		return errors.Wrap(err, "save member info")
	}

	return nil
}

// CheckLeader checks returns true if it is needed to check later.
func (m *Member) CheckLeader(ctx context.Context) (*Info, etcdutil.ModRevision, bool) {
	logger := m.lg

	if m.EtcdLeaderID() == 0 {
		logger.Info("no etcd leader, check PM leader later")
		time.Sleep(CheckAgainInterval)
		return nil, 0, true
	}

	leader, rev, err := m.GetLeader(ctx)
	if err != nil {
		logger.Warn("failed to get PM leader", zap.Error(err))
		time.Sleep(CheckAgainInterval)
		return nil, 0, true
	}

	if leader != nil && leader.MemberID == m.id {
		// oh, we are already a PM leader, which indicates we may meet something wrong
		// in previous CampaignLeader. We should delete the leadership and campaign again.
		logger.Warn("PM leader has not changed, delete and campaign again", zap.Object("old-pm-leader", leader))
		// Delete the leader itself and let others start a new election again.
		if err = m.leadership.DeleteLeaderKey(ctx); err != nil {
			logger.Warn("deleting PM leader key meets error", zap.Error(err))
			time.Sleep(CheckAgainInterval)
			return nil, 0, true
		}
		// Return nil and false to make sure the campaign will start immediately.
		return nil, 0, false
	}

	return leader, rev, false
}

// GetLeader gets the corresponding leader from etcd by given leaderPath (as the key).
func (m *Member) GetLeader(ctx context.Context) (*Info, etcdutil.ModRevision, error) {
	logger := m.lg

	kv, err := etcdutil.GetOne(ctx, m.client, []byte(m.LeaderPath()), logger)
	if err != nil {
		logger.Error("failed to get leader", zap.String("leader-key", m.LeaderPath()), zap.Error(err))
		return nil, 0, errors.Wrap(err, "get kv from etcd")
	}
	if kv == nil {
		return nil, 0, nil
	}

	info := &Info{}
	err = json.Unmarshal(kv.Value, info)
	if err != nil {
		logger.Error("failed to unmarshal leader info", zap.ByteString("raw-string", kv.Value), zap.Error(err))
		return nil, 0, errors.Wrap(err, "unmarshal leader info")
	}

	return info, kv.ModRevision, nil
}

// WatchLeader is used to watch the changes of the leader.
func (m *Member) WatchLeader(serverCtx context.Context, leader *Info, revision etcdutil.ModRevision) {
	m.setLeader(leader)
	m.leadership.Watch(serverCtx, revision)
	m.unsetLeader()
}

// CampaignLeader is used to campaign a PM member's leadership and make it become a PM leader.
// returns true if successfully campaign leader
func (m *Member) CampaignLeader(ctx context.Context, leaseTimeout int64) (bool, error) {
	return m.leadership.Campaign(ctx, leaseTimeout, string(m.Info()))
}

func (m *Member) Info() []byte {
	return m.infoValue
}

// KeepLeader is used to keep the PM leader's leadership.
func (m *Member) KeepLeader(ctx context.Context) {
	m.leadership.Keep(ctx)
}

// EnableLeader sets the member itself to a PM leader.
func (m *Member) EnableLeader() {
	m.setLeader(m.info)
}

// ResetLeader is used to reset the PM member's current leadership.
// Basically it will reset the leader lease and unset leader info.
func (m *Member) ResetLeader() {
	m.leadership.Reset()
	m.unsetLeader()
}

// CheckPriorityAndMoveLeader checks whether the etcd leader should be moved according to the priority, and moves if so
func (m *Member) CheckPriorityAndMoveLeader(ctx context.Context) error {
	etcdLeaderID := m.EtcdLeaderID()
	if etcdLeaderID == m.id || etcdLeaderID == 0 {
		return nil
	}
	logger := m.lg

	myPriority, err := m.GetMemberPriority(ctx, m.id)
	if err != nil {
		return errors.Wrap(err, "load current member priority")
	}
	leaderPriority, err := m.GetMemberPriority(ctx, etcdLeaderID)
	if err != nil {
		return errors.Wrap(err, "load etcd leader member priority")
	}

	if myPriority > leaderPriority {
		err := m.MoveEtcdLeader(ctx, etcdLeaderID, m.id)
		if err != nil {
			return errors.Wrap(err, "transfer etcd leader")
		}
		logger.Info("transfer etcd leader", zap.Uint64("from", etcdLeaderID), zap.Uint64("to", m.id))
	}
	return nil
}

// EtcdLeaderID returns current leaderID in etcd cluster
func (m *Member) EtcdLeaderID() uint64 {
	return m.etcd.Server.Lead()
}

// GetMemberPriority loads a member's priority to be elected as the etcd leader.
func (m *Member) GetMemberPriority(ctx context.Context, id uint64) (int, error) {
	logger := m.lg

	key := m.getPriorityPath(id)
	kv, err := etcdutil.GetOne(ctx, m.client, []byte(key), logger)
	if err != nil {
		return 0, errors.Wrapf(err, "failed to get member's leader priority by key %s", key)
	}
	if kv == nil {
		return 0, nil
	}

	priority, err := strconv.Atoi(string(kv.Value))
	if err != nil {
		return 0, errors.Wrap(err, "parse priority")
	}
	return priority, nil
}

// MoveEtcdLeader tries to transfer etcd leader.
func (m *Member) MoveEtcdLeader(ctx context.Context, old, new uint64) error {
	moveCtx, cancel := context.WithTimeout(ctx, _moveLeaderTimeout)
	defer cancel()

	logger := m.lg

	err := m.etcd.Server.MoveLeader(moveCtx, old, new)
	if err != nil {
		logger.Error("failed to move etcd leader", zap.Uint64("from", old), zap.Uint64("to", new), zap.Error(err))
		return errors.Wrap(err, "move leader")
	}
	return nil
}

// IsLeader returns whether current server is the leader
func (m *Member) IsLeader() bool {
	leader := m.Leader()
	return leader != nil && leader.MemberID == m.info.MemberID && m.leadership.Check()
}

// Leader returns current PM leader of PM cluster.
func (m *Member) Leader() *Info {
	leader := m.leader.Load()
	if leader == nil {
		return nil
	}
	if leader.MemberID == 0 {
		return nil
	}
	return leader
}

// ClusterInfo returns all members in the cluster.
func (m *Member) ClusterInfo(ctx context.Context) ([]*Info, error) {
	logger := m.lg.With(traceutil.TraceLogField(ctx))

	etcdMembers, err := m.client.MemberList(ctx)
	if err != nil {
		logger.Error("failed to list etcd members", zap.Error(err))
		return nil, errors.Wrap(err, "list etcd members")
	}

	leader := m.Leader()
	members := make([]*Info, 0, len(etcdMembers.Members))
	for _, em := range etcdMembers.Members {
		info, err := m.loadInfo(ctx, em.ID)
		if err != nil {
			return nil, errors.Wrapf(err, "get info of member %d", em.ID)
		}
		member := &Info{
			Name:            em.Name,
			MemberID:        em.ID,
			PeerUrls:        em.PeerURLs,
			ClientUrls:      em.ClientURLs,
			AdvertisePMAddr: info.AdvertisePMAddr,
		}
		if leader != nil && member.MemberID == leader.MemberID {
			member.IsLeader = true
		}
		members = append(members, member)
	}

	return members, nil
}

// Etcd returns etcd related information.
func (m *Member) Etcd() *embed.Etcd {
	return m.etcd
}

// ID returns the unique etcd ID for this server in etcd cluster.
func (m *Member) ID() uint64 {
	return m.id
}

func (m *Member) setLeader(member *Info) {
	m.leader.Store(member)
}

func (m *Member) unsetLeader() {
	m.leader.Store(&Info{})
}

func (m *Member) saveInfo(ctx context.Context, infoValue []byte) error {
	logger := m.lg

	txn := etcdutil.NewTxn(ctx, m.client, logger)
	resp, err := txn.Then(clientv3.OpPut(m.InfoPath(m.id), string(infoValue))).Commit()
	if err != nil {
		return errors.Wrap(err, "put member info")
	}
	if !resp.Succeeded {
		return errors.New("put member info: transaction failed")
	}

	return nil
}

func (m *Member) loadInfo(ctx context.Context, id uint64) (*Info, error) {
	logger := m.lg.With(zap.Uint64("member-id", id))

	key := m.InfoPath(id)
	kv, err := etcdutil.GetOne(ctx, m.client, []byte(key), logger)
	if err != nil {
		logger.Error("failed to get member info", zap.Error(err))
		return nil, errors.Wrapf(err, "failed to get member info by key %s", key)
	}
	if kv == nil {
		logger.Error("failed to get member info, key not found")
		return nil, errors.Errorf("failed to get member info: key not found: %s", key)
	}

	info := &Info{}
	err = json.Unmarshal(kv.Value, info)
	if err != nil {
		logger.Error("failed to unmarshal member info", zap.ByteString("raw-string", kv.Value), zap.Error(err))
		return nil, errors.Wrap(err, "unmarshal member info")
	}

	return info, nil
}

func (m *Member) LeaderPath() string {
	return path.Join(m.clusterRootPath, _leaderPathPrefix)
}

func (m *Member) getPriorityPath(id uint64) string {
	return path.Join(m.clusterRootPath, _memberPathPrefix, strconv.FormatUint(id, 10), _priorityPathPrefix)
}

func (m *Member) InfoPath(id uint64) string {
	return path.Join(m.clusterRootPath, _memberPathPrefix, strconv.FormatUint(id, 10), _infoPathPrefix)
}
