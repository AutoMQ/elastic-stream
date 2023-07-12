package client

import (
	"github.com/AutoMQ/pd/api/rpcfb/rpcfb"
	"github.com/AutoMQ/pd/pkg/sbp/protocol"
)

// newInResponse returns a new empty response for the given operation.
// It returns nil if the operation is not supported.
//
//nolint:gocritic
func newInResponse(op rpcfb.OperationCode) protocol.InResponse {
	switch op {
	default:
		return nil
	}
}
