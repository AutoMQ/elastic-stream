package config

import (
	"time"

	"github.com/pkg/errors"
	"github.com/spf13/pflag"
	"github.com/spf13/viper"
)

const (
	_defaultSealReqTimeoutMs   int32 = 1000
	_defaultRangeServerTimeout       = 100 * time.Second
)

// Cluster is the configuration for cluster.RaftCluster
type Cluster struct {
	SealReqTimeoutMs int32
	// RangeServerTimeout is the timeout after which a range server is considered dead.
	RangeServerTimeout time.Duration
}

func NewCluster() *Cluster {
	return &Cluster{}
}

func (c *Cluster) Validate() error {
	if c.SealReqTimeoutMs <= 0 {
		return errors.Errorf("invalid seal request timeout `%d`", c.SealReqTimeoutMs)
	}
	if c.RangeServerTimeout <= 0 {
		return errors.Errorf("invalid range server timeout `%s`", c.RangeServerTimeout)
	}
	return nil
}

func clusterConfigure(v *viper.Viper, fs *pflag.FlagSet) {
	fs.Int32("cluster-seal-req-timeout-ms", _defaultSealReqTimeoutMs, "seal request timeout in milliseconds")
	fs.Duration("cluster-range-server-timeout", _defaultRangeServerTimeout, "timeout after which a range server is considered dead")
	_ = v.BindPFlag("cluster.sealReqTimeoutMs", fs.Lookup("cluster-seal-req-timeout-ms"))
	_ = v.BindPFlag("cluster.rangeServerTimeout", fs.Lookup("cluster-range-server-timeout"))
}
