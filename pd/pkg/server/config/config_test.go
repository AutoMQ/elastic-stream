package config

import (
	"fmt"
	"io"
	"net/url"
	"os"
	"testing"
	"time"

	"github.com/spf13/pflag"
	"github.com/stretchr/testify/require"
	"go.etcd.io/etcd/server/v3/embed"
	"go.uber.org/zap"
	"go.uber.org/zap/zapcore"
)

var (
	_testDefaultLog = func() *Log {
		log := NewLog()
		log.Level = "INFO"
		log.Rotate.MaxSize = 64
		log.Rotate.MaxAge = 180
		return log
	}
	_testDefaultCluster = func() *Cluster {
		cluster := NewCluster()
		cluster.SealReqTimeoutMs = 1000
		cluster.RangeServerTimeout = 100 * time.Second
		return cluster
	}
	_testDefaultSbp = func() *Sbp {
		sbp := NewSbp()
		sbp.Server.HeartbeatInterval = 5 * time.Second
		sbp.Server.HeartbeatMissCount = 3
		sbp.Client.IdleConnTimeout = 0
		sbp.Client.ReadIdleTimeout = 5 * time.Second
		sbp.Client.HeartbeatTimeout = 10 * time.Second
		return sbp
	}
	_testDefaultConfig = func() Config {
		return Config{
			Etcd: func() *embed.Config {
				config := embed.NewConfig()
				config.InitialClusterToken = "pd-cluster"
				config.LogLevel = "warn"
				config.AutoCompactionMode = "periodic"
				config.AutoCompactionRetention = "1h"
				return config
			}(),
			Log:                         _testDefaultLog(),
			Cluster:                     _testDefaultCluster(),
			Sbp:                         _testDefaultSbp(),
			PeerUrls:                    "http://127.0.0.1:12380",
			ClientUrls:                  "http://127.0.0.1:12379",
			AdvertisePeerUrls:           "http://127.0.0.1:12380",
			AdvertiseClientUrls:         "http://127.0.0.1:12379",
			Name:                        "pd-hostname",
			DataDir:                     "default.pd-hostname",
			InitialCluster:              "pd-hostname=http://127.0.0.1:12380",
			PDAddr:                      "127.0.0.1:12378",
			AdvertisePDAddr:             "127.0.0.1:12378",
			LeaderLease:                 3,
			LeaderPriorityCheckInterval: time.Minute,
		}
	}

	_testCluster = func() *Cluster {
		cluster := NewCluster()
		cluster.SealReqTimeoutMs = 1234567
		cluster.RangeServerTimeout = 1*time.Hour + 1*time.Minute + 1*time.Second
		return cluster
	}
	_testSbp = func() *Sbp {
		sbp := NewSbp()
		sbp.Server.HeartbeatInterval = 2*time.Hour + 2*time.Minute + 2*time.Second
		sbp.Server.HeartbeatMissCount = 12345678
		sbp.Client.IdleConnTimeout = 3*time.Hour + 3*time.Minute + 3*time.Second
		sbp.Client.ReadIdleTimeout = 4*time.Hour + 4*time.Minute + 4*time.Second
		sbp.Client.HeartbeatTimeout = 5*time.Hour + 5*time.Minute + 5*time.Second
		return sbp
	}
	_testConfig = func() Config {
		return Config{
			Etcd: func() *embed.Config {
				config := embed.NewConfig()
				config.InitialClusterToken = "test-initial-cluster-token"
				config.LogLevel = "test-etcd-log-level"
				config.AutoCompactionMode = "test-auto-compaction-mode"
				config.AutoCompactionRetention = "test-auto-compaction-retention"
				config.TickMs = 123
				config.ElectionMs = 1234
				return config
			}(),
			Log: func() *Log {
				log := NewLog()
				log.Level = "FATAL"
				log.Zap.Level = zap.NewAtomicLevelAt(zapcore.FatalLevel)
				log.Zap.Encoding = "console"
				log.Zap.OutputPaths = []string{"stdout", "stderr"}
				log.Zap.ErrorOutputPaths = []string{"stdout", "stderr"}
				log.Zap.DisableCaller = true
				log.Zap.DisableStacktrace = true
				log.Zap.EncoderConfig.MessageKey = "test-msg"
				log.EnableRotation = false
				log.Rotate.MaxSize = 1234
				log.Rotate.MaxAge = 12345
				log.Rotate.MaxBackups = 123456
				log.Rotate.LocalTime = true
				log.Rotate.Compress = true
				return log
			}(),
			Cluster:                     _testCluster(),
			Sbp:                         _testSbp(),
			PeerUrls:                    "test-peer-urls",
			ClientUrls:                  "test-client-urls",
			AdvertisePeerUrls:           "test-advertise-peer-urls",
			AdvertiseClientUrls:         "test-advertise-client-urls",
			Name:                        "test-name",
			DataDir:                     "test-data-dir",
			InitialCluster:              "test-initial-cluster",
			PDAddr:                      "test-pd-addr",
			AdvertisePDAddr:             "test-advertise-pd-addr",
			LeaderLease:                 123,
			LeaderPriorityCheckInterval: time.Hour + time.Minute + time.Second,
		}
	}
)

func TestNewConfig(t *testing.T) {
	type args struct {
		arguments []string
	}
	tests := []struct {
		name    string
		args    args
		want    Config
		wantErr bool
		errMsg  string
	}{
		{
			name: "default config",
			args: args{arguments: []string{}},
			want: Config{
				Etcd: func() *embed.Config {
					config := embed.NewConfig()
					config.InitialClusterToken = "pd-cluster"
					config.LogLevel = "warn"
					config.AutoCompactionMode = "periodic"
					config.AutoCompactionRetention = "1h"
					return config
				}(),
				Log:                         _testDefaultLog(),
				Cluster:                     _testDefaultCluster(),
				Sbp:                         _testDefaultSbp(),
				PeerUrls:                    "http://127.0.0.1:12380",
				ClientUrls:                  "http://127.0.0.1:12379",
				AdvertisePeerUrls:           "",
				AdvertiseClientUrls:         "",
				Name:                        "",
				DataDir:                     "",
				InitialCluster:              "",
				PDAddr:                      "127.0.0.1:12378",
				AdvertisePDAddr:             "",
				LeaderLease:                 3,
				LeaderPriorityCheckInterval: time.Minute,
			},
		},
		{
			name: "default config in toml",
			args: args{arguments: []string{
				"--config=../../../conf/config.toml",
			}},
			want: _testDefaultConfig(),
		},
		{
			name: "default config in yaml",
			args: args{arguments: []string{
				"--config=../../../conf/config.yaml",
			}},
			want: _testDefaultConfig(),
		},
		{
			name: "config from command line (override config in file)",
			args: args{arguments: []string{
				"--config=../../../conf/config.toml",

				"--peer-urls=test-peer-urls",
				"--client-urls=test-client-urls",
				"--advertise-peer-urls=test-advertise-peer-urls",
				"--advertise-client-urls=test-advertise-client-urls",
				"--name=test-name",
				"--data-dir=test-data-dir",
				"--initial-cluster=test-initial-cluster",
				"--leader-lease=123",
				"--leader-priority-check-interval=1h1m1s",
				"--etcd-initial-cluster-token=test-initial-cluster-token",
				"--pd-addr=test-pd-addr",
				"--advertise-pd-addr=test-advertise-pd-addr",
				"--etcd-log-level=test-etcd-log-level",
				"--etcd-auto-compaction-mode=test-auto-compaction-mode",
				"--etcd-auto-compaction-retention=test-auto-compaction-retention",
				"--log-level=FATAL",
				"--log-zap-encoding=console",
				"--log-zap-output-paths=stdout,stderr",
				"--log-zap-error-output-paths=stdout,stderr",
				"--log-enable-rotation=false",
				"--log-rotate-max-size=1234",
				"--log-rotate-max-age=12345",
				"--log-rotate-max-backups=123456",
				"--log-rotate-local-time=true",
				"--log-rotate-compress=true",
				"--cluster-seal-req-timeout-ms=1234567",
				"--cluster-range-server-timeout=1h1m1s",
				"--sbp-server-heartbeat-interval=2h2m2s",
				"--sbp-server-heartbeat-miss-count=12345678",
				"--sbp-client-idle-conn-timeout=3h3m3s",
				"--sbp-client-read-idle-timeout=4h4m4s",
				"--sbp-client-heartbeat-timeout=5h5m5s",
			}},
			want: Config{
				Etcd: func() *embed.Config {
					config := embed.NewConfig()
					config.InitialClusterToken = "test-initial-cluster-token"
					config.LogLevel = "test-etcd-log-level"
					config.AutoCompactionMode = "test-auto-compaction-mode"
					config.AutoCompactionRetention = "test-auto-compaction-retention"
					return config
				}(),
				Log: func() *Log {
					log := NewLog()
					log.Level = "FATAL"
					log.Zap.Level = zap.NewAtomicLevelAt(zapcore.FatalLevel)
					log.Zap.Encoding = "console"
					log.Zap.OutputPaths = []string{"stdout", "stderr"}
					log.Zap.ErrorOutputPaths = []string{"stdout", "stderr"}
					log.EnableRotation = false
					log.Rotate.MaxSize = 1234
					log.Rotate.MaxAge = 12345
					log.Rotate.MaxBackups = 123456
					log.Rotate.LocalTime = true
					log.Rotate.Compress = true
					return log
				}(),
				Cluster:                     _testCluster(),
				Sbp:                         _testSbp(),
				PeerUrls:                    "test-peer-urls",
				ClientUrls:                  "test-client-urls",
				AdvertisePeerUrls:           "test-advertise-peer-urls",
				AdvertiseClientUrls:         "test-advertise-client-urls",
				Name:                        "test-name",
				DataDir:                     "test-data-dir",
				InitialCluster:              "test-initial-cluster",
				PDAddr:                      "test-pd-addr",
				AdvertisePDAddr:             "test-advertise-pd-addr",
				LeaderLease:                 123,
				LeaderPriorityCheckInterval: time.Hour + time.Minute + time.Second,
			},
		},
		{
			name: "config from toml file",
			args: args{arguments: []string{
				"--config=./test/test-config.toml",
			}},
			want: _testConfig(),
		},
		{
			name: "config from yaml file",
			args: args{arguments: []string{
				"--config=./test/test-config.yaml",
			}},
			want: _testConfig(),
		},
		{
			name: "help message",
			args: args{arguments: []string{
				"--help",
			}},
			wantErr: true,
			errMsg:  pflag.ErrHelp.Error(),
		},
		{
			name: "parse arguments error",
			args: args{arguments: []string{
				"--name=test",
				"--peer-urls",
			}},
			wantErr: true,
			errMsg:  "flag needs an argument",
		},
		{
			name: "read configuration file error",
			args: args{arguments: []string{
				"--config=not-exist.yaml",
			}},
			wantErr: true,
			errMsg:  "read configuration file",
		},
		{
			name: "unmarshal configuration error",
			args: args{arguments: []string{
				"--config=./test/test-invalid.toml",
			}},
			wantErr: true,
			errMsg:  "unmarshal configuration",
		},
		{
			name: "adjust log config error",
			args: args{arguments: []string{
				"--log-level=LEVEL",
			}},
			wantErr: true,
			errMsg:  "adjust log config",
		},
		{
			name: "create logger error",
			args: args{arguments: []string{
				"--log-zap-encoding=raw",
			}},
			wantErr: true,
			errMsg:  "build logger",
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			config, err := NewConfig(tt.args.arguments, io.Discard)

			if tt.wantErr {
				re.ErrorContains(err, tt.errMsg)
				return
			}
			re.NoError(err)

			// do not check auxiliary fields
			config.lg = nil

			equal(re, tt.want.Log.Zap, config.Log.Zap)
			tt.want.Log.Zap = zap.Config{}
			config.Log.Zap = zap.Config{}

			re.Equal(tt.want, *config)
		})
	}
}

func TestConfigFromEnv(t *testing.T) {
	re := require.New(t)

	t.Setenv("PD_NAME", "env-test-name")
	t.Setenv("PD_ETCD_INITIALCLUSTERTOKEN", "env-test-token")
	t.Setenv("PD_LOG_ZAP_ENCODERCONFIG_MESSAGEKEY", "env-test-msg-key")
	t.Setenv("PD_LOG_ZAP_DISABLECALLER", "true")
	t.Setenv("PD_ETCD_TICKMS", "4321")
	t.Setenv("PD_ETCD_CLUSTERSTATE", "env-test-cluster-state")

	config, err := NewConfig([]string{
		"--config=./test/test-config.toml",
		"--advertise-peer-urls=cmd-test-advertise-peer-urls",
	}, io.Discard)
	re.NoError(err)

	// flag > env > config > default
	re.Equal("cmd-test-advertise-peer-urls", config.AdvertisePeerUrls)
	re.Equal("env-test-name", config.Name)
	re.Equal("test-client-urls", config.ClientUrls)

	re.Equal("env-test-token", config.Etcd.InitialClusterToken)
	re.Equal("env-test-msg-key", config.Log.Zap.EncoderConfig.MessageKey)
	re.Equal(true, config.Log.Zap.DisableCaller)
	re.Equal(uint(4321), config.Etcd.TickMs)
	re.Equal("env-test-cluster-state", config.Etcd.ClusterState)
}

func TestConfig_Adjust(t *testing.T) {
	hostname, e := os.Hostname()
	require.NoError(t, e)

	tests := []struct {
		name    string
		in      *Config
		want    *Config
		wantErr bool
		errMsg  string
	}{
		{
			name: "default config",
			in:   func() *Config { config, _ := NewConfig([]string{}, io.Discard); return config }(),
			want: &Config{
				Etcd: func() *embed.Config {
					config := embed.NewConfig()
					config.Name = fmt.Sprintf("pd-%s", hostname)
					config.Dir = fmt.Sprintf("default.pd-%s", hostname)
					config.InitialCluster = fmt.Sprintf("pd-%s=http://127.0.0.1:12380", hostname)
					config.LPUrls, _ = parseUrls("http://127.0.0.1:12380")
					config.LCUrls, _ = parseUrls("http://127.0.0.1:12379")
					config.APUrls, _ = parseUrls("http://127.0.0.1:12380")
					config.ACUrls, _ = parseUrls("http://127.0.0.1:12379")
					config.InitialClusterToken = "pd-cluster"
					config.LogLevel = "warn"
					config.AutoCompactionMode = "periodic"
					config.AutoCompactionRetention = "1h"
					return config
				}(),
				Log:                         _testDefaultLog(),
				Cluster:                     _testDefaultCluster(),
				Sbp:                         _testDefaultSbp(),
				PeerUrls:                    "http://127.0.0.1:12380",
				ClientUrls:                  "http://127.0.0.1:12379",
				AdvertisePeerUrls:           "http://127.0.0.1:12380",
				AdvertiseClientUrls:         "http://127.0.0.1:12379",
				Name:                        fmt.Sprintf("pd-%s", hostname),
				DataDir:                     fmt.Sprintf("default.pd-%s", hostname),
				InitialCluster:              fmt.Sprintf("pd-%s=http://127.0.0.1:12380", hostname),
				PDAddr:                      "127.0.0.1:12378",
				AdvertisePDAddr:             "127.0.0.1:12378",
				LeaderLease:                 3,
				LeaderPriorityCheckInterval: time.Minute,
			},
		},
		{
			name: "normal adjust config",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				config.PeerUrls = "http://example.com:12380,http://10.0.0.1:12380"
				config.ClientUrls = "http://example.com:12379,http://10.0.0.1:12379"
				config.PDAddr = "example.com:12378"
				config.Name = "test-name"
				return config
			}(),
			want: &Config{
				Etcd: func() *embed.Config {
					config := embed.NewConfig()
					config.Name = "test-name"
					config.Dir = "default.test-name"
					config.InitialCluster = "test-name=http://example.com:12380,test-name=http://10.0.0.1:12380"
					config.LPUrls, _ = parseUrls("http://example.com:12380,http://10.0.0.1:12380")
					config.LCUrls, _ = parseUrls("http://example.com:12379,http://10.0.0.1:12379")
					config.APUrls, _ = parseUrls("http://example.com:12380,http://10.0.0.1:12380")
					config.ACUrls, _ = parseUrls("http://example.com:12379,http://10.0.0.1:12379")
					config.InitialClusterToken = "pd-cluster"
					config.LogLevel = "warn"
					config.AutoCompactionMode = "periodic"
					config.AutoCompactionRetention = "1h"
					return config
				}(),
				Log:                         _testDefaultLog(),
				Cluster:                     _testDefaultCluster(),
				Sbp:                         _testDefaultSbp(),
				PeerUrls:                    "http://example.com:12380,http://10.0.0.1:12380",
				ClientUrls:                  "http://example.com:12379,http://10.0.0.1:12379",
				AdvertisePeerUrls:           "http://example.com:12380,http://10.0.0.1:12380",
				AdvertiseClientUrls:         "http://example.com:12379,http://10.0.0.1:12379",
				Name:                        "test-name",
				DataDir:                     "default.test-name",
				InitialCluster:              "test-name=http://example.com:12380,test-name=http://10.0.0.1:12380",
				PDAddr:                      "example.com:12378",
				AdvertisePDAddr:             "example.com:12378",
				LeaderLease:                 3,
				LeaderPriorityCheckInterval: time.Minute,
			},
		},
		{
			name: "invalid peer url",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				config.PeerUrls = "http://example.com:12380,://10.0.0.1:12380"
				return config
			}(),
			wantErr: true,
			errMsg:  "parse peer url",
		},
		{
			name: "invalid client url",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				config.ClientUrls = "http://example.com:12379,://10.0.0.1:12379"
				return config
			}(),
			wantErr: true,
			errMsg:  "parse client url",
		},
		{
			name: "invalid advertise peer url",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				config.AdvertisePeerUrls = "http://example.com:12380,://10.0.0.1:12380"
				return config
			}(),
			wantErr: true,
			errMsg:  "parse advertise peer url",
		},
		{
			name: "invalid advertise client url",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				config.AdvertiseClientUrls = "http://example.com:12379,://10.0.0.1:12379"
				return config
			}(),
			wantErr: true,
			errMsg:  "parse advertise client url",
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			config := tt.in
			err := config.Adjust()

			if tt.wantErr {
				re.ErrorContains(err, tt.errMsg)
				return
			}
			re.NoError(err)

			// do not check auxiliary fields
			config.lg = nil

			equal(re, tt.want.Log.Zap, config.Log.Zap)
			tt.want.Log.Zap = zap.Config{}
			config.Log.Zap = zap.Config{}

			re.Equal(tt.want, config)
		})
	}
}

func equal(re *require.Assertions, wantZap zap.Config, actualZap zap.Config) {
	re.Equal(wantZap.Level.String(), actualZap.Level.String())
	re.Equal(wantZap.Encoding, actualZap.Encoding)
	re.Equal(wantZap.OutputPaths, actualZap.OutputPaths)
	re.Equal(wantZap.ErrorOutputPaths, actualZap.ErrorOutputPaths)
	re.Equal(wantZap.Development, actualZap.Development)
	re.Equal(wantZap.DisableStacktrace, actualZap.DisableStacktrace)
	re.Equal(wantZap.DisableCaller, actualZap.DisableCaller)
}

func TestConfig_Validate(t *testing.T) {
	tests := []struct {
		name    string
		in      *Config
		wantErr bool
		errMsg  string
	}{
		{
			name: "default config",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				return config
			}(),
		},
		{
			name: "invalid Cluster.SealReqTimeoutMs",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				config.Cluster.SealReqTimeoutMs = 0
				return config
			}(),
			wantErr: true,
			errMsg:  "validate cluster config: invalid seal request timeout",
		},
		{
			name: "invalid Cluster.RangeServerTimeout",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				config.Cluster.RangeServerTimeout = 0
				return config
			}(),
			wantErr: true,
			errMsg:  "validate cluster config: invalid range server timeout",
		},
		{
			name: "invalid Sbp.Server.HeartbeatInterval",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				config.Sbp.Server.HeartbeatInterval = -1
				return config
			}(),
			wantErr: true,
			errMsg:  "validate sbp config: invalid heartbeat interval",
		},
		{
			name: "invalid Sbp.Server.HeartbeatMissCount",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				config.Sbp.Server.HeartbeatMissCount = -1
				return config
			}(),
			wantErr: true,
			errMsg:  "validate sbp config: invalid heartbeat miss count",
		},
		{
			name: "invalid Sbp.Client.IdleConnTimeout",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				config.Sbp.Client.IdleConnTimeout = -1
				return config
			}(),
			wantErr: true,
			errMsg:  "validate sbp config: invalid idle connection timeout",
		},
		{
			name: "invalid Sbp.Client.ReadIdleTimeout",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				config.Sbp.Client.ReadIdleTimeout = -1
				return config
			}(),
			wantErr: true,
			errMsg:  "validate sbp config: invalid read idle timeout",
		},
		{
			name: "invalid Sbp.Client.HeartbeatTimeout",
			in: func() *Config {
				config, _ := NewConfig([]string{}, io.Discard)
				config.Sbp.Client.HeartbeatTimeout = 0
				return config
			}(),
			wantErr: true,
			errMsg:  "validate sbp config: invalid heartbeat timeout",
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			err := tt.in.Validate()

			if tt.wantErr {
				re.ErrorContains(err, tt.errMsg)
				return
			}
			re.NoError(err)
		})
	}
}

func TestParseUrls(t *testing.T) {
	type args struct {
		s string
	}
	tests := []struct {
		name    string
		args    args
		want    []url.URL
		wantErr bool
	}{
		{
			name: "normal case",
			args: args{s: "http://localhost:12380,https://example.com,ftp://10.0.0.1:12380"},
			want: []url.URL{
				{
					Scheme: "http",
					Host:   "localhost:12380",
				},
				{
					Scheme: "https",
					Host:   "example.com",
				},
				{
					Scheme: "ftp",
					Host:   "10.0.0.1:12380",
				},
			},
		},
		{
			name:    "bad case",
			args:    args{s: "://localhost:12380,https://example.com,ftp://10.0.0.1:12380"},
			wantErr: true,
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			urls, err := parseUrls(tt.args.s)

			if tt.wantErr {
				re.Error(err)
				return
			}
			re.NoError(err)
			re.Equal(tt.want, urls)
		})
	}
}
