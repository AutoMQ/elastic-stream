package config

import (
	"time"

	"github.com/pkg/errors"
	"github.com/spf13/pflag"
	"github.com/spf13/viper"
)

const (
	_defaultSealReqTimeoutMs int32 = 1000
	_defaultDataNodeTimeout        = 100 * time.Second
)

// Cluster is the configuration for cluster.RaftCluster
type Cluster struct {
	SealReqTimeoutMs int32
	// DataNodeTimeout is the timeout after which a data node is considered dead.
	DataNodeTimeout time.Duration
}

func NewCluster() *Cluster {
	return &Cluster{}
}

func (c *Cluster) Validate() error {
	if c.SealReqTimeoutMs <= 0 {
		return errors.Errorf("invalid seal request timeout `%d`", c.SealReqTimeoutMs)
	}
	if c.DataNodeTimeout <= 0 {
		return errors.Errorf("invalid data node timeout `%s`", c.DataNodeTimeout)
	}
	return nil
}

func clusterConfigure(v *viper.Viper, fs *pflag.FlagSet) {
	fs.Int32("cluster-seal-req-timeout-ms", _defaultSealReqTimeoutMs, "seal request timeout in milliseconds")
	fs.Duration("cluster-data-node-timeout", _defaultDataNodeTimeout, "timeout after which a data node is considered dead")
	_ = v.BindPFlag("cluster.sealReqTimeoutMs", fs.Lookup("cluster-seal-req-timeout-ms"))
	_ = v.BindPFlag("cluster.dataNodeTimeout", fs.Lookup("cluster-data-node-timeout"))
}
