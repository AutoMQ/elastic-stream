//nolint:unused
package server

import (
	"context"
	"net"
	"sync"
	"sync/atomic"
	"time"

	"github.com/pkg/errors"
	"go.uber.org/zap"
)

// ErrServerClosed is returned by the Server's Serve and ListenAndServe methods
// after a call to Shutdown or Close.
var ErrServerClosed = errors.New("Server closed")

// Handler responds to a request
type Handler interface {

	// Heartbeat is used to keep clients alive
	Heartbeat(clientID string)
}

// A Server is used to serve requests from the Elastic Storage
type Server struct {

	// IdleTimeout specifies how long until idle clients should be
	// closed with a GOAWAY frame. PING frames are not considered
	// activity for the purposes of IdleTimeout.
	// TODO move into a config
	IdleTimeout time.Duration

	shuttingDown atomic.Bool
	handler      Handler

	ctx context.Context
	lg  *zap.Logger

	mu          sync.Mutex
	listeners   map[*net.Listener]struct{}
	activeConns map[*conn]struct{}
	doneChan    chan struct{}

	listenerGroup sync.WaitGroup
	connGroup     sync.WaitGroup
}

// NewServer creates a server
func NewServer(ctx context.Context, handler Handler, logger *zap.Logger) *Server {
	return &Server{
		ctx:     ctx,
		handler: handler,
		lg:      logger,
	}
}

// Serve accepts incoming connections on the Listener l, creating a
// new service goroutine for each. The service goroutines read requests and
// then call s.Handler to reply to them.
//
// Serve always returns a non-nil error and closes l.
// After Shutdown or Close, the returned error is ErrServerClosed.
func (s *Server) Serve(l net.Listener) error {
	l = &onceCloseListener{Listener: l}
	defer func() { _ = l.Close() }()

	if !s.trackListener(&l, true) {
		return ErrServerClosed
	}
	defer s.trackListener(&l, false)

	logger := s.lg
	var tempDelay time.Duration // how long to sleep on accept failure
	for {
		rw, err := l.Accept()
		if err != nil {
			select {
			case <-s.getDoneChan():
				return ErrServerClosed
			default:
			}
			if ne, ok := err.(net.Error); ok && ne.Timeout() {
				if tempDelay == 0 {
					tempDelay = 5 * time.Millisecond
				} else {
					tempDelay *= 2
				}
				if max := 1 * time.Second; tempDelay > max {
					tempDelay = max
				}
				logger.Error("listener accept failed", zap.Duration("retry-in", tempDelay), zap.Error(err))
				time.Sleep(tempDelay)
				continue
			}
			return err
		}
		tempDelay = 0
		logger.Info("start connection", zap.String("remote-addr", rw.RemoteAddr().String()))
		c := newConn(s, rw)
		s.trackConn(c, true)

		go c.serve(s.ctx)
	}
}

// Shutdown gracefully shuts down the server without interrupting any
// active connections. Shutdown works by first closing all open
// listeners, then gracefully close all connections.
// If the provided context expires before the shutdown is complete,
// Shutdown returns the context's error, otherwise it returns any
// error returned from closing the Server's underlying Listener(s).
//
// When Shutdown is called, Serve immediately return ErrServerClosed.
// Make sure the program doesn't exit and waits for Shutdown to return.
//
// Once Shutdown has been called on a server, it may not be reused;
// future calls to methods such as Serve will return ErrServerClosed.
func (s *Server) Shutdown(ctx context.Context) error {
	logger := s.lg
	if !s.shuttingDown.Swap(true) {
		logger.Warn("server is already shutting down")
		return nil
	}

	logger.Info("start to close sbp server")
	s.mu.Lock()
	// close listeners
	err := s.closeListenersLocked()
	// notify server to break serve loop
	s.closeDoneChanLocked()
	s.mu.Unlock()
	s.listenerGroup.Wait()

	// notify connections to break serve loop
	s.startGracefulShutdown()

	c := make(chan struct{})
	go func() {
		defer close(c)
		s.connGroup.Wait()
	}()
	select {
	case <-c:
	case <-ctx.Done():
		err = ctx.Err()
	}

	logger.Info("sbp server closed", zap.Error(err))
	return err
}

// trackListener adds or removes a net.Listener to the set of tracked
// listeners.
//
// We store a pointer to interface in the map set, in case the
// net.Listener is not comparable. This is safe because we only call
// trackListener via Serve and can track+defer untrack the same
// pointer to local variable there. We never need to compare a
// Listener from another caller.
//
// It reports whether the server is still up (not Shutdown or Closed).
func (s *Server) trackListener(ln *net.Listener, add bool) bool {
	logger := s.lg
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.listeners == nil {
		s.listeners = make(map[*net.Listener]struct{})
	}
	if add {
		if s.isShuttingDown() {
			return false
		}
		logger.Info("add listener", zap.String("addr", (*ln).Addr().String()))
		s.listeners[ln] = struct{}{}
		s.listenerGroup.Add(1)
	} else {
		logger.Info("delete listener", zap.String("addr", (*ln).Addr().String()))
		delete(s.listeners, ln)
		s.listenerGroup.Done()
	}
	return true
}

func (s *Server) isShuttingDown() bool {
	return s.shuttingDown.Load()
}

func (s *Server) trackConn(c *conn, add bool) {
	logger := s.lg
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.activeConns == nil {
		s.activeConns = make(map[*conn]struct{})
	}
	if add {
		logger.Info("add conn", zap.String("addr", c.rwc.RemoteAddr().String()))
		s.activeConns[c] = struct{}{}
		s.connGroup.Add(1)
	} else {
		logger.Info("delete conn", zap.String("addr", c.rwc.RemoteAddr().String()))
		delete(s.activeConns, c)
		s.connGroup.Done()
	}
}

func (s *Server) getDoneChan() <-chan struct{} {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.getDoneChanLocked()
}

func (s *Server) getDoneChanLocked() chan struct{} {
	if s.doneChan == nil {
		s.doneChan = make(chan struct{})
	}
	return s.doneChan
}

func (s *Server) closeDoneChanLocked() {
	ch := s.getDoneChanLocked()
	select {
	case <-ch:
		// Already closed. Don't close again.
	default:
		// Safe to close here. We're the only closer, guarded
		// by s.mu.
		close(ch)
	}
}

func (s *Server) closeListenersLocked() error {
	var err error
	for ln := range s.listeners {
		if cerr := (*ln).Close(); cerr != nil && err == nil {
			err = cerr
		}
	}
	return err
}

func (s *Server) startGracefulShutdown() {
	s.mu.Lock()
	for c := range s.activeConns {
		c.startGracefulShutdown()
	}
	s.mu.Unlock()
}

// onceCloseListener wraps a net.Listener, protecting it from
// multiple Close calls.
type onceCloseListener struct {
	net.Listener
	once     sync.Once
	closeErr error
}

func (oc *onceCloseListener) Close() error {
	oc.once.Do(oc.close)
	return oc.closeErr
}

func (oc *onceCloseListener) close() {
	oc.closeErr = oc.Listener.Close()
}
