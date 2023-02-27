//nolint:unused
package server

import (
	"context"
	"net"
	"sync"
	"time"

	"github.com/AutoMQ/placement-manager/pkg/elasticstorage/codec"
)

// conn is the state of a connection between server and client.
type conn struct {
	// Immutable:
	server *Server
	rwc    net.Conn

	ctx         context.Context
	framer      *codec.Framer
	doneServing chan struct{}        // closed when serverConn.serve ends
	readFrameCh chan readFrameResult // written by serverConn.readFrames

	// Everything following is owned by the serve loop; use serveG.check():
	// serveG            goroutineLock // used to verify funcs are on serve()
	maxClientStreamID uint32 // max ever seen from client (odd), or 0 if there have been no client requests
	maxPushPromiseID  uint32 // ID of the last push promise (even), or 0 if there have been no pushes
	streams           map[uint32]*stream
	inGoAway          bool        // we've started to or sent GOAWAY
	needToSendGoAway  bool        // we need to schedule a GOAWAY frame write
	shutdownTimer     *time.Timer // nil until used
	idleTimer         *time.Timer // nil if unused

	// Used by startGracefulShutdown.
	shutdownOnce sync.Once
}

func newConn(srv *Server, rwc net.Conn) *conn {
	return &conn{
		server: srv,
		rwc:    rwc,
	}
}

func (c *conn) serve(ctx context.Context) {
	// TODO
	_ = ctx
}

func (c *conn) close() {
	// TODO
	c.server.trackConn(c, false)
	close(c.doneServing)
	_ = c.rwc.Close()
}

func (c *conn) startGracefulShutdown() {
	// TODO
}

type readFrameResult struct {
	f   codec.Frame
	err error
}
