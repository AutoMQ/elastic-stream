package server

import (
	"sync"

	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/sbp/codec"
	"github.com/AutoMQ/pd/third_party/forked/eapache/queue"
)

type frameQueue = queue.Queue[frameWriteRequest]

// writeScheduler manages frames to be written in each stream
// Methods are never called concurrently.
type writeScheduler struct {
	// Frames in ctrlQueue queue are control frames, and should be popped first.
	ctrlQueue *frameQueue

	// queues contains the stream-specific queues, keyed by stream ID.
	// When a stream is idle, closed, or emptied, it's deleted
	// from the map.
	queues map[uint32]*frameQueue

	// queuePool is pool of empty queues for reuse.
	queuePool sync.Pool
}

// newWriteScheduler creates a new writeScheduler with empty queues
func newWriteScheduler() *writeScheduler {
	ws := &writeScheduler{
		queues: make(map[uint32]*frameQueue),
		queuePool: sync.Pool{
			New: func() interface{} {
				return queue.New[frameWriteRequest]()
			},
		},
	}
	ws.ctrlQueue = ws.queuePool.Get().(*frameQueue)
	return ws
}

// Push queues a frame in the scheduler.
func (ws *writeScheduler) Push(wr frameWriteRequest) {
	if isControl(wr.f) {
		ws.ctrlQueue.Add(wr)
		return
	}
	id := wr.stream.id
	q, ok := ws.queues[id]
	if !ok {
		q = ws.queuePool.Get().(*frameQueue)
		ws.queues[id] = q
	}
	q.Add(wr)
}

func isControl(f codec.Frame) bool {
	return f.Base().OpCode == rpcfb.OperationCodeGOAWAY
}

// Pop dequeues the next frame to write. Returns false if no frames can
// be written. Frames with a given wr.stream.id are Pop'd in the same
// order they are Push'd. No frames should be discarded except by CloseStream.
func (ws *writeScheduler) Pop() (frameWriteRequest, bool) {
	if !ws.ctrlQueue.IsEmpty() {
		return ws.ctrlQueue.Remove(), true
	}
	for id, q := range ws.queues {
		wr := q.Remove()
		if q.IsEmpty() {
			ws.deleteQueue(q, id)
		}
		return wr, true
	}
	return frameWriteRequest{}, false
}

// CloseStream closes a stream in the write scheduler. Any frames queued on
// this stream should be discarded.
func (ws *writeScheduler) CloseStream(streamID uint32) {
	q, ok := ws.queues[streamID]
	if !ok {
		return
	}
	ws.deleteQueue(q, streamID)
}

func (ws *writeScheduler) deleteQueue(q *frameQueue, streamID uint32) {
	delete(ws.queues, streamID)
	q.Reset()
	ws.queuePool.Put(q)
}

// frameWriteRequest is a request to write a frame.
type frameWriteRequest struct {
	f codec.Frame

	// free should be called once the frame is no longer needed
	free func()

	// stream is the stream on which this frame will be written.
	stream *stream

	// done, if non-nil, must be a buffered channel with space for
	// 1 message and is sent the return value from write (or an
	// earlier error) when the frame has been written.
	done chan error

	// whether f is the last frame to be written in the stream
	endStream bool
}

// replyToWriter sends err to frameWriteRequest.done and panics if done is unbuffered
// This does nothing if frameWriteRequest.done is nil.
func (wr *frameWriteRequest) replyToWriter(err error) {
	if wr.done == nil {
		return
	}
	select {
	case wr.done <- err:
	default:
		panic("unbuffered done channel")
	}
}
