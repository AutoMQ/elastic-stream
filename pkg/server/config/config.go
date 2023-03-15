package config

import (
	"fmt"
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

	_envPrefix = "PM"

	_defaultPeerUrls                          = "http://127.0.0.1:2380"
	_defaultClientUrls                        = "http://127.0.0.1:2379"
	_defaultCompactionMode                    = "periodic"
	_defaultAutoCompactionRetention           = "1h"
	_defaultNameFormat                        = "pm-%s"
	_defaultDataDirFormat                     = "default.%s"
	_defaultInitialClusterPrefix              = "pm="
	_defaultInitialClusterToken               = "pm-cluster"
	_defaultSbpAddr                           = "127.0.0.1:2378"
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
	v *viper.Viper

	Etcd *embed.Config
	Log  *Log

	PeerUrls            string
	ClientUrls          string
	AdvertisePeerUrls   string
	AdvertiseClientUrls string

	Name           string
	DataDir        string
	InitialCluster string
	SbpAddr        string

	// LeaderLease time, if leader doesn't update its TTL
	// in etcd after lease time, etcd will expire the leader key
	// and other servers can campaign the leader again.
	// Etcd only supports seconds TTL, so here is second too.
	LeaderLease int64

	LeaderPriorityCheckInterval time.Duration

	lg *zap.Logger
}

// NewConfig creates a new config.
func NewConfig(arguments []string) (*Config, error) {
	cfg := &Config{}
	cfg.Etcd = embed.NewConfig()
	cfg.Log = NewLog()

	v, fs := configure()
	cfg.v = v

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
		logger.Info("load configuration from file", zap.String("file-name", configFile))
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
		// For example, when AdvertisePeerUrls is set to "http://127.0.0.1:2380,http://127.0.0.1:2381",
		// the InitialCluster is "pm=http://127.0.0.1:2380,pm=http://127.0.0.1:2381".
		items := strings.Split(c.AdvertisePeerUrls, URLSeparator)
		initialCluster := strings.Join(items, URLSeparator+_defaultInitialClusterPrefix)
		c.InitialCluster += _defaultInitialClusterPrefix + initialCluster
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
		return errors.Wrap(err, "invalid data dir path")
	}
	return nil
}

// Logger returns logger generated based on the config
// It can be used after calling NewConfig
func (c *Config) Logger() *zap.Logger {
	return c.lg
}

func configure() (*viper.Viper, *pflag.FlagSet) {
	v := viper.New()
	fs := pflag.NewFlagSet("placement-manager", pflag.ContinueOnError)

	v.SetEnvKeyReplacer(strings.NewReplacer(".", "_", "-", "_"))
	v.AllowEmptyEnv(true)
	v.SetEnvPrefix(_envPrefix)
	v.AutomaticEnv()

	// Viper settings
	for _, filePath := range _defaultConfigFilePaths {
		v.AddConfigPath(filePath)
	}

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
	fs.String("etcd-auto-compaction-mode", _defaultCompactionMode, "interpret 'auto-compaction-retention' one of: periodic|revision. 'periodic' for duration based retention, defaulting to hours if no time unit is provided (e.g. '5m'). 'revision' for revision number based retention.")
	fs.String("etcd-auto-compaction-retention", _defaultAutoCompactionRetention, "auto compaction retention for mvcc key value store. 0 means disable auto compaction.")
	_ = v.BindPFlag("etcd.autoCompactionMode", fs.Lookup("etcd-auto-compaction-mode"))
	_ = v.BindPFlag("etcd.autoCompactionRetention", fs.Lookup("etcd-auto-compaction-retention"))

	// PM members settings
	fs.String("name", "", "human-readable name for this PM member (default 'pm-${hostname}')")
	fs.String("data-dir", "", "path to the data directory (default 'default.${name}')")
	fs.String("initial-cluster", "", "initial cluster configuration for bootstrapping, e.g. pm=http://127.0.0.1:2380. (default 'pm=${advertise-peer-urls}')")
	fs.Int64("leader-lease", _defaultLeaderLease, "expiration time of the leader, in seconds")
	fs.Duration("leader-priority-check-interval", _defaultLeaderPriorityCheckInterval, "time interval for checking the leader's priority")
	fs.String("etcd-initial-cluster-token", _defaultInitialClusterToken, "set different tokens to prevent communication between PMs in different clusters")
	fs.String("sbp-addr", _defaultSbpAddr, "the address of sbp server")
	_ = v.BindPFlag("name", fs.Lookup("name"))
	_ = v.BindPFlag("dataDir", fs.Lookup("data-dir"))
	_ = v.BindPFlag("initialCluster", fs.Lookup("initial-cluster"))
	_ = v.BindPFlag("leaderLease", fs.Lookup("leader-lease"))
	_ = v.BindPFlag("leaderPriorityCheckInterval", fs.Lookup("leader-priority-check-interval"))
	_ = v.BindPFlag("etcd.initialClusterToken", fs.Lookup("etcd-initial-cluster-token"))
	_ = v.BindPFlag("sbpAddr", fs.Lookup("sbp-addr"))

	// log settings
	fs.String("log-level", _defaultLogLevel, "the minimum enabled logging level")
	fs.StringSlice("log-zap-output-paths", _defaultLogZapOutputPaths, "a list of URLs or file paths to write logging output to")
	fs.StringSlice("log-zap-error-output-paths", []string{}, "a list of URLs to write internal logger errors to (default ${log-zap-output-paths})")
	fs.String("log-zap-encoding", _defaultLogZapEncoding, "the logger's encoding, \"json\" or \"console\"")
	fs.Bool("log-enable-rotation", _defaultLogEnableRotation, "whether to enable log rotation")
	fs.Int("log-rotate-max-size", _defaultLogRotateMaxSize, "maximum size in megabytes of the log file before it gets rotated")
	fs.Int("log-rotate-max-age", _defaultLogRotateMaxAge, "maximum number of days to retain old log files based on the timestamp encoded in their filename")
	fs.Int("log-rotate-max-backups", _defaultLogRotateMaxBackups, "maximum number of old log files to retain, default is to retain all old log files (though MaxAge may still cause them to get deleted)")
	fs.Bool("log-rotate-local-time", _defaultLogRotateLocalTime, "whether the time used for formatting the timestamps in backup files is the computer's local time, default is to use UTC time")
	fs.Bool("log-rotate-compress", _defaultLogRotateCompress, "whether the rotated log files should be compressed using gzip")
	_ = v.BindPFlag("log.level", fs.Lookup("log-level"))
	_ = v.BindPFlag("log.zap.outputPaths", fs.Lookup("log-zap-output-paths"))
	_ = v.BindPFlag("log.zap.errorOutputPaths", fs.Lookup("log-zap-error-output-paths"))
	_ = v.BindPFlag("log.zap.encoding", fs.Lookup("log-zap-encoding"))
	_ = v.BindPFlag("log.enableRotation", fs.Lookup("log-enable-rotation"))
	_ = v.BindPFlag("log.rotate.maxSize", fs.Lookup("log-rotate-max-size"))
	_ = v.BindPFlag("log.rotate.maxAge", fs.Lookup("log-rotate-max-age"))
	_ = v.BindPFlag("log.rotate.maxBackups", fs.Lookup("log-rotate-max-backups"))
	_ = v.BindPFlag("log.rotate.localTime", fs.Lookup("log-rotate-local-time"))
	_ = v.BindPFlag("log.rotate.compress", fs.Lookup("log-rotate-compress"))

	// bind env not set before
	_ = v.BindEnv("etcd.clusterState")

	return v, fs
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
