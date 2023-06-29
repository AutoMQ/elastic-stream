package config

import (
	"time"

	"github.com/pkg/errors"
	"github.com/spf13/pflag"
	"github.com/spf13/viper"
)

const (
	_defaultHeartbeatInterval = 5 * time.Second

	_defaultServerHeartbeatInterval  = _defaultHeartbeatInterval
	_defaultServerHeartbeatMissCount = 3

	_defaultClientIdleConnTimeout  = 0
	_defaultClientReadIdleTimeout  = _defaultHeartbeatInterval
	_defaultClientHeartbeatTimeout = 10 * time.Second
)

// Sbp is the configuration for the SBP server and client
type Sbp struct {
	Server *SbpServer
	Client *SbpClient
}

func NewSbp() *Sbp {
	return &Sbp{
		Server: &SbpServer{},
		Client: &SbpClient{},
	}
}

func (s *Sbp) Validate() error {
	if s.Server.HeartbeatInterval < 0 {
		return errors.Errorf("invalid heartbeat interval `%s`", s.Server.HeartbeatInterval)
	}
	if s.Server.HeartbeatMissCount < 0 {
		return errors.Errorf("invalid heartbeat miss count `%d`", s.Server.HeartbeatMissCount)
	}
	if s.Client.IdleConnTimeout < 0 {
		return errors.Errorf("invalid idle connection timeout `%s`", s.Client.IdleConnTimeout)
	}
	if s.Client.ReadIdleTimeout < 0 {
		return errors.Errorf("invalid read idle timeout `%s`", s.Client.ReadIdleTimeout)
	}
	if s.Client.HeartbeatTimeout <= 0 {
		return errors.Errorf("invalid heartbeat timeout `%s`", s.Client.HeartbeatTimeout)
	}
	return nil
}

func sbpConfigure(v *viper.Viper, fs *pflag.FlagSet) {
	fs.Duration("sbp-server-heartbeat-interval", _defaultServerHeartbeatInterval, "time interval between sending heartbeats from client to server")
	_ = v.BindPFlag("sbp.server.heartbeatInterval", fs.Lookup("sbp-server-heartbeat-interval"))
	fs.Int("sbp-server-heartbeat-miss-count", _defaultServerHeartbeatMissCount, "number of consecutive heartbeats that the server can miss before considering the client to be unresponsive and terminating the connection")
	_ = v.BindPFlag("sbp.server.heartbeatMissCount", fs.Lookup("sbp-server-heartbeat-miss-count"))

	fs.Duration("sbp-client-idle-conn-timeout", _defaultClientIdleConnTimeout, "time after which an idle connection closes itself (zero for no timeout)")
	_ = v.BindPFlag("sbp.client.idleConnTimeout", fs.Lookup("sbp-client-idle-conn-timeout"))
	fs.Duration("sbp-client-read-idle-timeout", _defaultClientReadIdleTimeout, "time after which a health check will be carried out (zero for no health checks)")
	_ = v.BindPFlag("sbp.client.readIdleTimeout", fs.Lookup("sbp-client-read-idle-timeout"))
	fs.Duration("sbp-client-heartbeat-timeout", _defaultClientHeartbeatTimeout, "time after which the client closes the connection if the server doesn't respond to a heartbeat request")
	_ = v.BindPFlag("sbp.client.heartbeatTimeout", fs.Lookup("sbp-client-heartbeat-timeout"))
}

// SbpServer is the configuration for the SBP server
type SbpServer struct {
	// HeartbeatInterval defines the interval duration between sending heartbeats from client to server.
	HeartbeatInterval time.Duration
	// HeartbeatMissCount is the number of consecutive heartbeats that the server can miss
	// before considering the client to be unresponsive and terminating the connection.
	HeartbeatMissCount int
}

// SbpClient is the configuration for the SBP client
type SbpClient struct {
	// IdleConnTimeout is the maximum amount of time an idle (keep-alive) connection
	// will remain idle before closing itself.
	// If zero, no idle connections are closed.
	IdleConnTimeout time.Duration
	// ReadIdleTimeout is the timeout after which a health check using Heartbeat
	// frame will be carried out if no frame is received on the connection.
	// If zero, no health check is performed.
	ReadIdleTimeout time.Duration
	// HeartbeatTimeout is the timeout after which the connection will be closed
	// if a response to Heartbeat is not received.
	HeartbeatTimeout time.Duration
}
