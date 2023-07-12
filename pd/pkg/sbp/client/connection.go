package client

import (
	"context"
	"io"
	"math"
	"net"
	"sync"
	"time"

	"github.com/bytedance/gopkg/lang/mcache"
	"github.com/pkg/errors"
	"go.uber.org/zap"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/sbp/codec"
	"github.com/AutoMQ/pd/pkg/sbp/protocol"
)

var (
	errClientConnGotGoAway = errors.New("sbp: received Server's graceful shutdown GOAWAY")
)

// conn is the state of a single client connection to a server.
type conn struct {
	c    *SbpClient
	conn net.Conn

	// readLoop goroutine fields:
	readerDone chan struct{} // closed on error
	readerErr  error         // set before readerDone is closed

	idleTimeout time.Duration // or 0 for never
	idleTimer   *time.Timer

	mu              sync.Mutex // guards following
	cond            *sync.Cond // hold mu; broadcast on stream closed
	closing         bool
	closed          bool
	goAway          *codec.GoAwayFrame       // if non-nil, the GoAwayFrame we received
	streams         map[uint32]*stream       // client-initiated
	heartbeats      map[uint32]chan struct{} // in flight heartbeat stream ID to notification channel
	streamsReserved int                      // incr by reserveNewRequest; decr on roundTrip
	nextStreamID    uint32
	lastActive      time.Time

	// reqMu is a 1-element semaphore channel controlling access to sending new requests.
	// Write to reqHeaderMu to lock it, read from it to unlock.
	// Lock reqMu BEFORE mu or wmu.
	reqMu chan struct{}

	// wmu is held while writing.
	wmu  sync.Mutex
	fr   *codec.Framer
	werr error // first write error that has occurred

	lg *zap.Logger
}

func (cc *conn) roundTrip(req protocol.OutRequest) (protocol.InResponse, error) {
	ctx := req.Context()
	s := cc.newSteam(ctx)
	go s.doRequest(req)

	for {
		select {
		case <-s.respRcv:
			return s.res, nil
		case <-s.abort:
			// wait done
			select {
			case <-s.donec:
			case <-ctx.Done():
			}
			return nil, s.abortErr
		case <-ctx.Done():
			err := ctx.Err()
			s.abortStream(err)
			return nil, err
		}
	}
}

func (cc *conn) newSteam(ctx context.Context) *stream {
	s := &stream{
		cc:      cc,
		ctx:     ctx,
		abort:   make(chan struct{}),
		respEnd: make(chan struct{}),
		donec:   make(chan struct{}),
		respRcv: make(chan struct{}),
	}
	return s
}

func (cc *conn) reserveNewRequest() bool {
	cc.mu.Lock()
	defer cc.mu.Unlock()

	canTakeNewRequest := cc.goAway == nil && !cc.closed && !cc.closing && cc.nextStreamID < math.MaxInt32
	if !canTakeNewRequest {
		return false
	}
	cc.streamsReserved++
	return true
}

// readLoop runs in its own goroutine and reads and dispatches frames.
func (cc *conn) readLoop() {
	rl := &connReadLoop{cc: cc}
	defer rl.cleanup()
	cc.readerErr = rl.run()
	// TODO check readErr and send GoAway optionally
}

func (cc *conn) decrStreamReservations() {
	cc.mu.Lock()
	defer cc.mu.Unlock()
	cc.decrStreamReservationsLocked()
}

func (cc *conn) decrStreamReservationsLocked() {
	if cc.streamsReserved > 0 {
		cc.streamsReserved--
	}
}

func (cc *conn) addStreamLocked(s *stream) {
	s.id = cc.nextStreamID
	cc.nextStreamID++
	cc.streams[s.id] = s
}

func (cc *conn) forgetStreamID(id uint32) {
	logger := cc.lg

	cc.mu.Lock()
	sLen := len(cc.streams)
	delete(cc.streams, id)
	if len(cc.streams) != sLen-1 {
		logger.Error("forgetting a stream that doesn't exist", zap.Uint32("stream-id", id))
		return
	}

	cc.lastActive = time.Now()
	if len(cc.streams) == 0 && cc.idleTimer != nil {
		cc.idleTimer.Reset(cc.idleTimeout)
	}

	if cc.goAway != nil && cc.streamsReserved == 0 && len(cc.streams) == 0 {
		logger.Info("closing conn after sending goaway", zap.Uint32("max-stream-id", cc.nextStreamID-1))
		cc.closed = true
		defer cc.closeConn()
	}

	cc.cond.Broadcast()
	cc.mu.Unlock()
}

func (cc *conn) heartbeat(ctx context.Context) error {
	fmt := cc.c.format()
	req := protocol.HeartbeatRequest{
		HeartbeatRequestT: rpcfb.HeartbeatRequestT{
			ClientId:   cc.c.id,
			ClientRole: rpcfb.ClientRoleCLIENT_ROLE_PD,
		},
	}
	header, err := req.Marshal(fmt)
	defer func() {
		if header != nil {
			mcache.Free(header)
		}
	}()
	if err != nil {
		return err
	}

	c := make(chan struct{})
	cc.mu.Lock()
	id := cc.nextStreamID
	cc.nextStreamID++
	cc.heartbeats[id] = c
	cc.mu.Unlock()

	f := codec.NewHeartbeatFrameReq(id, fmt, header)

	errc := make(chan error, 1)
	go func() {
		cc.wmu.Lock()
		defer cc.wmu.Unlock()
		if err := cc.fr.WriteFrame(f); err != nil {
			errc <- err
			return
		}
		if err := cc.fr.Flush(); err != nil {
			errc <- err
			return
		}
	}()
	select {
	case <-c:
		return nil
	case err := <-errc:
		return err
	case <-ctx.Done():
		return ctx.Err()
	case <-cc.readerDone:
		// connection closed
		return cc.readerErr
	}
}

// shutdown gracefully closes the connection, waiting for running streams to complete.
func (cc *conn) shutdown(ctx context.Context) error {
	if err := cc.sendGoAway(); err != nil {
		return err
	}
	// Wait for all in-flight streams to complete or connection to close
	done := make(chan struct{})
	cancelled := false // guarded by cc.mu
	go func() {
		cc.mu.Lock()
		defer cc.mu.Unlock()
		for {
			if len(cc.streams) == 0 || cc.closed {
				cc.closed = true
				close(done)
				break
			}
			if cancelled {
				break
			}
			cc.cond.Wait()
		}
	}()
	select {
	case <-done:
		cc.closeConn()
		return nil
	case <-ctx.Done():
		cc.mu.Lock()
		// Free the goroutine above
		cancelled = true
		cc.cond.Broadcast()
		cc.mu.Unlock()
		return ctx.Err()
	}
}

func (cc *conn) sendGoAway() error {
	cc.mu.Lock()
	closing := cc.closing
	cc.closing = true
	maxStreamID := cc.nextStreamID
	cc.mu.Unlock()
	if closing {
		// GOAWAY sent already
		return nil
	}

	cc.wmu.Lock()
	defer cc.wmu.Unlock()
	// Send a graceful shutdown frame to server
	f := codec.NewGoAwayFrame(maxStreamID, false)
	if err := cc.fr.WriteFrame(f); err != nil {
		return err
	}
	if err := cc.fr.Flush(); err != nil {
		return err
	}
	// Prevent new requests
	return nil
}

func (cc *conn) healthCheck() {
	logger := cc.lg
	heartbeatTimeout := cc.c.heartbeatTimeout()
	// We don't need to periodically ping in the health check, because the readLoop of ClientConn will
	// trigger the healthCheck again if there is no frame received.
	ctx, cancel := context.WithTimeout(context.Background(), heartbeatTimeout)
	defer cancel()
	err := cc.heartbeat(ctx)
	if err != nil {
		logger.Warn("health check failed", zap.Error(err))
		cc.closeForLostHeartbeat()
	}
}

// onIdleTimeout is called from a time.AfterFunc goroutine. It will
// only be called when we're idle, but because we're coming from a new
// goroutine, there could be a new request coming in at the same time,
// so this simply calls the synchronized closeIfIdle to shut down this
// connection. The timer could just call closeIfIdle, but this is more
// clear.
func (cc *conn) onIdleTimeout() {
	cc.closeIfIdle()
}

func (cc *conn) closeIfIdle() {
	cc.mu.Lock()
	if len(cc.streams) > 0 || cc.streamsReserved > 0 {
		cc.mu.Unlock()
		return
	}

	cc.closed = true
	nextID := cc.nextStreamID
	cc.mu.Unlock()

	cc.lg.Info("closing idle conn", zap.Uint32("max-stream-id", nextID-1))
	cc.closeConn()
}

// Close closes the client connection immediately.
//
// In-flight requests are interrupted. For a graceful shutdown, use Shutdown instead.
func (cc *conn) close() {
	err := errors.New("sbp: client connection force closed")
	cc.closeForError(err)
}

// closes the client connection immediately. In-flight requests are interrupted.
func (cc *conn) closeForLostHeartbeat() {
	err := errors.New("sbp: client connection heartbeat lost")
	cc.closeForError(err)
}

// closes the client connection immediately. In-flight requests are interrupted.
// err is sent to streams.
func (cc *conn) closeForError(err error) {
	cc.mu.Lock()
	cc.closed = true
	for _, cs := range cc.streams {
		cs.abortStreamLocked(err)
	}
	cc.cond.Broadcast()
	cc.mu.Unlock()
	cc.closeConn()
}

func (cc *conn) closeConn() {
	_ = cc.conn.Close()
}

func (cc *conn) setGoAway(f *codec.GoAwayFrame) {
	cc.mu.Lock()
	defer cc.mu.Unlock()

	cc.goAway = f
	last := f.StreamID
	for streamID, s := range cc.streams {
		if streamID > last {
			s.abortStreamLocked(errClientConnGotGoAway)
		}
	}
}

// connReadLoop is the state owned by conn.readLoop.
type connReadLoop struct {
	cc *conn
}

func (rl *connReadLoop) run() error {
	logger := rl.cc.lg
	cc := rl.cc

	readIdleTimeout := cc.c.cfg.ReadIdleTimeout
	var t *time.Timer
	if readIdleTimeout != 0 {
		t = time.AfterFunc(readIdleTimeout, cc.healthCheck)
		defer t.Stop()
	}

	for {
		f, free, err := cc.fr.ReadFrame()
		if t != nil {
			t.Reset(readIdleTimeout)
		}
		if err != nil {
			logger.Debug("read frame failed", zap.Error(err))
			// TODO Check stream errors and only close the steam
			return err
		}

		switch f := f.(type) {
		case *codec.PingFrame:
			err = rl.processPing(f)
		case *codec.GoAwayFrame:
			err = rl.processGoAway(f)
		case *codec.DataFrame:
			if f.OpCode == rpcfb.OperationCodeHEARTBEAT {
				err = rl.processHeartbeat(f)
			} else {
				err = rl.processData(f)
			}
		default:
			logger.Warn("client ignoring unknown type frame", f.Info()...)
		}
		if free != nil {
			free()
		}
		if err != nil {
			info := f.Info()
			info = append(info, zap.Error(err))
			logger.Error("process frame failed", info...)
			return err
		}
	}
}

func (rl *connReadLoop) processPing(f *codec.PingFrame) error {
	if f.IsResponse() {
		return nil
	}

	ping, free := codec.NewPingFrameResp(f)
	defer free()

	cc := rl.cc
	cc.wmu.Lock()
	defer cc.wmu.Unlock()

	if err := cc.fr.WriteFrame(ping); err != nil {
		return err
	}
	return cc.fr.Flush()
}

func (rl *connReadLoop) processGoAway(f *codec.GoAwayFrame) error {
	cc := rl.cc
	logger := cc.lg
	if f.IsResponse() {
		// no need to deal with a GOAWAY response
		return nil
	}

	logger.Info("client received goaway", zap.Uint32("stream-id", f.StreamID))
	cc.c.connPool.markDead(cc)
	cc.setGoAway(f)
	return nil
}

func (rl *connReadLoop) processHeartbeat(f *codec.DataFrame) error {
	cc := rl.cc
	logger := cc.lg
	if f.IsRequest() {
		logger.Warn("client ignoring heartbeat request", f.Info()...)
		return nil
	}

	cc.mu.Lock()
	defer cc.mu.Unlock()
	// notify listener if any
	if c, ok := cc.heartbeats[f.StreamID]; ok {
		close(c)
		delete(cc.heartbeats, f.StreamID)
	}
	return nil
}

func (rl *connReadLoop) processData(f *codec.DataFrame) error {
	cc := rl.cc
	logger := cc.lg
	if f.IsRequest() {
		logger.Warn("client ignoring request data frame", f.Info()...)
		return nil
	}

	s := rl.streamByID(f.StreamID)
	if s == nil {
		// We'd get here if we canceled a request while the server had its response still in flight.
		// So if this was just something we canceled, ignore it.
		return nil
	}
	if s.readClosed {
		logger.Warn("client received data after RESPONSE_END", f.Info()...)
		rl.endStreamError(s, errors.New("sbp: protocol error: response after RESPONSE_END"))
		return nil
	}

	res, err := rl.parseResponse(f)
	if err != nil {
		logger.Warn("client failed to parse response", f.Info()...)
		rl.endStreamError(s, err)
		return nil
	}

	s.res = res
	close(s.respRcv)
	if f.IsResponseEnd() {
		rl.endStream(s)
	}
	return nil
}

func (rl *connReadLoop) parseResponse(f *codec.DataFrame) (resp protocol.InResponse, err error) {
	if f.IsSystemError() {
		resp = &protocol.SystemErrorResponse{}
	} else {
		resp = newInResponse(f.OpCode)
	}

	if resp == nil {
		return nil, errors.Errorf("sbp: unknown response opcode: %s", f.OpCode)
	}

	err = resp.Unmarshal(f.HeaderFmt, f.Header)
	if err != nil {
		return nil, errors.Wrap(err, "unmarshal response")
	}
	return
}

func (rl *connReadLoop) streamByID(id uint32) *stream {
	rl.cc.mu.Lock()
	defer rl.cc.mu.Unlock()
	s := rl.cc.streams[id]
	if s != nil && !s.readAborted {
		return s
	}
	return nil
}

func (rl *connReadLoop) endStream(s *stream) {
	if !s.readClosed {
		s.readClosed = true
		close(s.respEnd)
	}
}

func (rl *connReadLoop) endStreamError(s *stream, err error) {
	s.readAborted = true
	s.abortStream(err)
}

func (rl *connReadLoop) cleanup() {
	cc := rl.cc

	cc.c.connPool.markDead(cc)
	defer cc.closeConn()
	defer close(cc.readerDone)

	if cc.idleTimer != nil {
		cc.idleTimer.Stop()
	}

	err := cc.readerErr
	cc.mu.Lock()
	defer cc.mu.Unlock()
	if cc.goAway != nil && isEOFOrNetReadError(err) {
		err = errors.Errorf("sbp: server sent GOAWAY and closed the connection, lastStreamID = %d", cc.goAway.StreamID)
	} else if err == io.EOF {
		err = io.ErrUnexpectedEOF
	}
	cc.closed = true

	for _, s := range cc.streams {
		select {
		case <-s.respEnd:
			// The server closed the stream before closing the connection,
			// so no need to interrupt it.
		default:
			s.abortStreamLocked(err)
		}
	}
	cc.cond.Broadcast()
}

func isEOFOrNetReadError(err error) bool {
	if err == io.EOF {
		return true
	}
	ne, ok := err.(*net.OpError)
	return ok && ne.Op == "read"
}
