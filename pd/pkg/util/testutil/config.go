//go:build testing

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

package testutil

import (
	"fmt"
	"io"
	"net/url"
	"testing"

	"github.com/stretchr/testify/require"
	"go.etcd.io/etcd/server/v3/embed"

	"github.com/AutoMQ/pd/pkg/server/config"
	tempurl "github.com/AutoMQ/pd/pkg/util/testutil/url"
)

// NewEtcdConfig is used to create an etcd config for the unit test purpose.
func NewEtcdConfig(tb testing.TB) *embed.Config {
	cfg := embed.NewConfig()
	cfg.Name = "test_etcd"
	cfg.Dir = tb.TempDir()
	cfg.WalDir = ""
	cfg.Logger = "zap"
	cfg.LogOutputs = []string{"stdout"}

	pu, _ := url.Parse(tempurl.Alloc(tb))
	cfg.LPUrls = []url.URL{*pu}
	cfg.APUrls = cfg.LPUrls
	cu, _ := url.Parse(tempurl.Alloc(tb))
	cfg.LCUrls = []url.URL{*cu}
	cfg.ACUrls = cfg.LCUrls

	cfg.StrictReconfigCheck = false
	cfg.InitialCluster = fmt.Sprintf("%s=%s", cfg.Name, &cfg.LPUrls[0])
	cfg.ClusterState = embed.ClusterStateFlagNew

	cfg.LogLevel = "error"
	return cfg
}

func NewPDConfig(tb testing.TB) *config.Config {
	re := require.New(tb)

	peerURL := tempurl.Alloc(tb)
	clientURL := tempurl.Alloc(tb)
	pdAddr := tempurl.AllocAddr(tb)

	cfg, err := config.NewConfig([]string{
		"--peer-urls", peerURL,
		"--client-urls", clientURL,
		"--pd-addr", pdAddr,
	}, io.Discard)
	re.NoError(err)

	cfg.Name = fmt.Sprintf("test_pd_%s", tb.Name())
	cfg.DataDir = tb.TempDir()

	return cfg
}
