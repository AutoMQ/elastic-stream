package client

import (
	"context"
	"sync"

	"github.com/bytedance/gopkg/lang/mcache"

	"github.com/AutoMQ/pd/pkg/sbp/codec"
	"github.com/AutoMQ/pd/pkg/sbp/protocol"
)

// stream is the state for a single stream
type stream struct {
	cc *conn

	ctx context.Context

	id uint32

	abortOnce sync.Once
	abort     chan struct{} // closed to signal stream should end immediately
	abortErr  error         // set if abort is closed

	respEnd chan struct{} // closed when the peer sends an RESPONSE_END flag
	donec   chan struct{} // closed after the stream is in the closed state

	// owned by connReadLoop:
	readClosed  bool // peer sent an RESPONSE_END flag
	readAborted bool // read loop reset the stream

	respRcv chan struct{}       // closed when response is received
	res     protocol.InResponse // set if respRcv is closed
}

// doRequest runs for the duration of the request lifetime.
func (s *stream) doRequest(req protocol.OutRequest) {
	err := s.writeRequest(req)
	s.cleanupWriteRequest(err)
}

func (s *stream) writeRequest(req protocol.OutRequest) error {
	cc := s.cc
	ctx := s.ctx

	select {
	case cc.reqMu <- struct{}{}:
	case <-ctx.Done():
		return ctx.Err()
	}

	// register stream to conn
	cc.mu.Lock()
	if cc.idleTimer != nil {
		cc.idleTimer.Stop()
	}
	cc.decrStreamReservationsLocked()
	cc.addStreamLocked(s) // assigns stream ID
	cc.mu.Unlock()

	err := s.encodeAndWrite(req)
	<-cc.reqMu
	if err != nil {
		return err
	}

	select {
	case <-s.respEnd:
		return nil
	case <-s.abort:
		return s.abortErr
	case <-ctx.Done():
		return ctx.Err()
	}
}

func (s *stream) encodeAndWrite(req protocol.OutRequest) error {
	cc := s.cc
	ctx := s.ctx

	cc.wmu.Lock()
	defer cc.wmu.Unlock()

	// If the request was canceled while waiting for cc.wmu, just quit.
	select {
	case <-s.abort:
		return s.abortErr
	case <-ctx.Done():
		return ctx.Err()
	default:
	}

	header, err := req.Marshal(cc.c.format())
	defer func() {
		if header != nil {
			mcache.Free(header)
		}
	}()
	if err != nil {
		return err
	}
	fCtx := &codec.DataFrameContext{
		OpCode:    req.Operation(),
		HeaderFmt: cc.c.format(),
		StreamID:  s.id,
	}
	f := codec.NewDataFrameReq(fCtx, header, nil)
	err = cc.fr.WriteFrame(f)

	if err != nil && cc.werr == nil {
		cc.werr = err
	}
	err = cc.fr.Flush()
	if err != nil && cc.werr == nil {
		cc.werr = err
	}

	return cc.werr
}

// cleanupWriteRequest performs post-request tasks.
func (s *stream) cleanupWriteRequest(err error) {
	cc := s.cc

	if s.id == 0 {
		// We were canceled before creating the stream, so return our reservation.
		cc.decrStreamReservations()
	}

	if err != nil {
		s.abortStream(err)
	}
	if s.id != 0 {
		cc.forgetStreamID(s.id)
	}

	cc.wmu.Lock()
	werr := cc.werr
	cc.wmu.Unlock()
	if werr != nil {
		cc.close()
	}

	close(s.donec)
}

func (s *stream) abortStream(err error) {
	s.cc.mu.Lock()
	defer s.cc.mu.Unlock()

	s.abortStreamLocked(err)
}

func (s *stream) abortStreamLocked(err error) {
	s.abortOnce.Do(func() {
		s.abortErr = err
		close(s.abort)
	})
	s.cc.cond.Broadcast()
}
