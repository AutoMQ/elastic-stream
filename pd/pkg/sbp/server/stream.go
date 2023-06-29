package server

type streamState int

const (
	stateOpen streamState = iota
	stateClosed
)

// stream is the state for a single stream
type stream struct {
	cc *conn

	id uint32

	state streamState
}
