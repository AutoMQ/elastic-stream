//go:build testing

package testutil

import (
	"testing"

	"github.com/stretchr/testify/require"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.etcd.io/etcd/server/v3/embed"
	"go.uber.org/zap"
)

func StartEtcd(tb testing.TB, cfg *embed.Config) (*embed.Etcd, *clientv3.Client, func()) {
	re := require.New(tb)

	// start etcd
	if cfg == nil {
		cfg = NewEtcdConfig(tb)
	}
	etcd, err := embed.StartEtcd(cfg)
	re.NoError(err)

	// new client
	ep := cfg.LCUrls[0].String()
	client, err := clientv3.New(clientv3.Config{
		Endpoints: []string{ep},
		Logger:    zap.NewNop(),
	})
	re.NoError(err)

	<-etcd.Server.ReadyNotify()

	return etcd, client, func() { _ = client.Close(); etcd.Close() }
}
