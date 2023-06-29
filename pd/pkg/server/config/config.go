package config

import (
	"fmt"
	"io"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/pkg/errors"
	"github.com/spf13/pflag"
	"github.com/spf13/viper"
	"go.etcd.io/etcd/server/v3/embed"
	"go.uber.org/zap"
)

var (
	_defaultConfigFilePaths   = []string{".", "$CONFIG_DIR/"}
	_defaultLogZapOutputPaths = []string{"stderr"}
)

const (
	URLSeparator = "," // URLSeparator is the separator in fields such as PeerUrls, ClientUrls, etc.

	_envPrefix = "PD"

	_defaultPeerUrls                          = "http://127.0.0.1:12380"
	_defaultClientUrls                        = "http://127.0.0.1:12379"
	_defaultEtcdLogLevel                      = "warn"
	_defaultCompactionMode                    = "periodic"
	_defaultAutoCompactionRetention           = "1h"
	_defaultNameFormat                        = "pd-%s"
	_defaultDataDirFormat                     = "default.%s"
	_defaultInitialClusterFormat              = "%s=%s"
	_defaultInitialClusterToken               = "pd-cluster"
	_defaultPDAddr                            = "127.0.0.1:12378"
	_defaultLeaderLease                 int64 = 3
	_defaultLeaderPriorityCheckInterval       = time.Minute

	_defaultLogLevel            = "INFO"
	_defaultLogZapEncoding      = "json"
	_defaultLogEnableRotation   = false
	_defaultLogRotateMaxSize    = 64
	_defaultLogRotateMaxAge     = 180
	_defaultLogRotateMaxBackups = 0
	_defaultLogRotateLocalTime  = false
	_defaultLogRotateCompress   = false
)

// Config is the configuration for [Server]
type Config struct {
	Etcd    *embed.Config
	Log     *Log
	Cluster *Cluster
	Sbp     *Sbp

	PeerUrls            string
	ClientUrls          string
	AdvertisePeerUrls   string
	AdvertiseClientUrls string

	Name            string
	DataDir         string
	InitialCluster  string
	PDAddr          string
	AdvertisePDAddr string

	// LeaderLease time, if leader doesn't update its TTL
	// in etcd after lease time, etcd will expire the leader key
	// and other servers can campaign the leader again.
	// Etcd only supports seconds TTL, so here is second too.
	LeaderLease int64

	LeaderPriorityCheckInterval time.Duration

	lg *zap.Logger
}

// NewConfig creates a new config.
func NewConfig(arguments []string, errOutput io.Writer) (*Config, error) {
	cfg := &Config{}
	cfg.Etcd = embed.NewConfig()
	cfg.Log = NewLog()
	cfg.Cluster = NewCluster()
	cfg.Sbp = NewSbp()

	v := newViper()
	fs := newFlagSet(errOutput)
	configure(v, fs)

	// parse from command line
	fs.String("config", "", "configuration file")
	err := fs.Parse(arguments)
	if err != nil {
		return nil, err
	}

	// read configuration from file
	c, _ := fs.GetString("config")
	v.SetConfigFile(c)
	err = v.ReadInConfig()
	if err != nil {
		if _, ok := err.(viper.ConfigFileNotFoundError); !ok {
			return nil, errors.Wrap(err, "read configuration file")
		}
	}

	// set config
	err = v.Unmarshal(cfg)
	if err != nil {
		return nil, errors.Wrap(err, "unmarshal configuration")
	}

	// new and set logger (first thing after configuration loaded)
	err = cfg.Log.Adjust()
	if err != nil {
		return nil, errors.Wrap(err, "adjust log config")
	}
	logger, err := cfg.Log.Logger()
	if err != nil {
		return nil, errors.Wrap(err, "create logger")
	}
	cfg.lg = logger

	if configFile := v.ConfigFileUsed(); configFile != "" {
		logger.Debug("load configuration from file", zap.String("file-name", configFile))
	}

	return cfg, nil
}

// Adjust generates default values for some fields (if they are empty)
func (c *Config) Adjust() error {
	if c.AdvertisePeerUrls == "" {
		c.AdvertisePeerUrls = c.PeerUrls
	}
	if c.AdvertiseClientUrls == "" {
		c.AdvertiseClientUrls = c.ClientUrls
	}
	if c.Name == "" {
		hostname, err := os.Hostname()
		if err != nil {
			return errors.Wrap(err, "get hostname")
		}
		c.Name = fmt.Sprintf(_defaultNameFormat, hostname)
	}
	if c.DataDir == "" {
		c.DataDir = fmt.Sprintf(_defaultDataDirFormat, c.Name)
	}
	if c.InitialCluster == "" {
		// For example, when Name is set "my-pd" and AdvertisePeerUrls is set to "http://127.0.0.1:12380,http://127.0.0.1:12381",
		// the InitialCluster is "my-pd=http://127.0.0.1:12380,my-pd=http://127.0.0.1:12381".
		urls := strings.Split(c.AdvertisePeerUrls, URLSeparator)
		nodes := make([]string, 0, len(urls))
		for _, u := range urls {
			nodes = append(nodes, fmt.Sprintf(_defaultInitialClusterFormat, c.Name, u))
		}
		c.InitialCluster = strings.Join(nodes, URLSeparator)
	}
	if c.AdvertisePDAddr == "" {
		c.AdvertisePDAddr = c.PDAddr
	}

	// set etcd config
	err := c.adjustEtcd()
	if err != nil {
		return errors.Wrap(err, "adjust etcd config")
	}

	return nil
}

func (c *Config) adjustEtcd() error {
	cfg := c.Etcd
	cfg.Name = c.Name
	cfg.Dir = c.DataDir
	cfg.InitialCluster = c.InitialCluster
	// cfg.EnablePprof = true

	var err error
	cfg.LPUrls, err = parseUrls(c.PeerUrls)
	if err != nil {
		return errors.Wrap(err, "parse peer url")
	}
	cfg.LCUrls, err = parseUrls(c.ClientUrls)
	if err != nil {
		return errors.Wrap(err, "parse client url")
	}
	cfg.APUrls, err = parseUrls(c.AdvertisePeerUrls)
	if err != nil {
		return errors.Wrap(err, "parse advertise peer url")
	}
	cfg.ACUrls, err = parseUrls(c.AdvertiseClientUrls)
	if err != nil {
		return errors.Wrap(err, "parse advertise client url")
	}

	return nil
}

// Validate checks whether the configuration is valid. It should be called after Adjust
func (c *Config) Validate() error {
	_, err := filepath.Abs(c.DataDir)
	if err != nil {
		return errors.Wrapf(err, "invalid data dir path `%s`", c.DataDir)
	}

	if err := c.Cluster.Validate(); err != nil {
		return errors.Wrap(err, "validate cluster config")
	}

	if err := c.Sbp.Validate(); err != nil {
		return errors.Wrap(err, "validate sbp config")
	}

	return nil
}

// Logger returns logger generated based on the config
// It can be used after calling NewConfig
func (c *Config) Logger() *zap.Logger {
	if c != nil {
		return c.lg
	}
	return nil
}

func newViper() *viper.Viper {
	v := viper.New()
	v.SetEnvKeyReplacer(strings.NewReplacer(".", "_", "-", "_"))
	v.AllowEmptyEnv(true)
	v.SetEnvPrefix(_envPrefix)
	v.AutomaticEnv()
	for _, filePath := range _defaultConfigFilePaths {
		v.AddConfigPath(filePath)
	}
	return v
}

func newFlagSet(errOutput io.Writer) *pflag.FlagSet {
	fs := pflag.NewFlagSet("pd", pflag.ContinueOnError)
	fs.SetOutput(errOutput)
	return fs
}

func configure(v *viper.Viper, fs *pflag.FlagSet) {
	// etcd urls settings
	fs.String("peer-urls", _defaultPeerUrls, "urls for peer traffic")
	fs.String("client-urls", _defaultClientUrls, "urls for client traffic")
	fs.String("advertise-peer-urls", "", "advertise urls for peer traffic (default '${peer-urls}')")
	fs.String("advertise-client-urls", "", "advertise urls for client traffic (default '${client-urls}')")
	_ = v.BindPFlag("peerUrls", fs.Lookup("peer-urls"))
	_ = v.BindPFlag("clientUrls", fs.Lookup("client-urls"))
	_ = v.BindPFlag("advertisePeerUrls", fs.Lookup("advertise-peer-urls"))
	_ = v.BindPFlag("advertiseClientUrls", fs.Lookup("advertise-client-urls"))

	// other etcd settings
	fs.String("etcd-log-level", _defaultEtcdLogLevel, "log level for etcd. One of: debug|info|warn|error|panic|fatal")
	fs.String("etcd-auto-compaction-mode", _defaultCompactionMode, "interpret 'auto-compaction-retention' one of: periodic|revision. 'periodic' for duration based retention, defaulting to hours if no time unit is provided (e.g. '5m'). 'revision' for revision number based retention.")
	fs.String("etcd-auto-compaction-retention", _defaultAutoCompactionRetention, "auto compaction retention for mvcc key value store. 0 means disable auto compaction.")
	_ = v.BindPFlag("etcd.logLevel", fs.Lookup("etcd-log-level"))
	_ = v.BindPFlag("etcd.autoCompactionMode", fs.Lookup("etcd-auto-compaction-mode"))
	_ = v.BindPFlag("etcd.autoCompactionRetention", fs.Lookup("etcd-auto-compaction-retention"))

	// PD members settings
	fs.String("name", "", "human-readable name for this PD member (default 'pd-${hostname}')")
	fs.String("data-dir", "", "path to the data directory (default 'default.${name}')")
	fs.String("initial-cluster", "", "initial cluster configuration for bootstrapping, e.g. pd=http://127.0.0.1:12380. (default '${name}=${advertise-peer-urls}')")
	fs.Int64("leader-lease", _defaultLeaderLease, "expiration time of the leader, in seconds")
	fs.Duration("leader-priority-check-interval", _defaultLeaderPriorityCheckInterval, "time interval for checking the leader's priority")
	fs.String("etcd-initial-cluster-token", _defaultInitialClusterToken, "set different tokens to prevent communication between PD nodes in different clusters")
	fs.String("pd-addr", _defaultPDAddr, "the address of PD")
	fs.String("advertise-pd-addr", "", "advertise address of PD (default '${pd-addr}')")
	_ = v.BindPFlag("name", fs.Lookup("name"))
	_ = v.BindPFlag("dataDir", fs.Lookup("data-dir"))
	_ = v.BindPFlag("initialCluster", fs.Lookup("initial-cluster"))
	_ = v.BindPFlag("leaderLease", fs.Lookup("leader-lease"))
	_ = v.BindPFlag("leaderPriorityCheckInterval", fs.Lookup("leader-priority-check-interval"))
	_ = v.BindPFlag("etcd.initialClusterToken", fs.Lookup("etcd-initial-cluster-token"))
	_ = v.BindPFlag("pdAddr", fs.Lookup("pd-addr"))
	_ = v.BindPFlag("advertisePDAddr", fs.Lookup("advertise-pd-addr"))

	// bind env not set before
	_ = v.BindEnv("etcd.clusterState")

	logConfigure(v, fs)
	clusterConfigure(v, fs)
	sbpConfigure(v, fs)
}

// parseUrls parse a string into multiple urls.
func parseUrls(s string) ([]url.URL, error) {
	items := strings.Split(s, URLSeparator)
	urls := make([]url.URL, 0, len(items))
	for _, item := range items {
		u, err := url.Parse(item)
		if err != nil {
			return nil, errors.Wrapf(err, "parse url %s", item)
		}

		urls = append(urls, *u)
	}

	return urls, nil
}
