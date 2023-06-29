package randutil

import (
	"crypto/rand"

	"github.com/pkg/errors"

	"github.com/AutoMQ/pd/pkg/util/typeutil"
)

// Uint64 returns a random 64-bit value as a uint64
func Uint64() (uint64, error) {
	bytes := make([]byte, 8)

	_, err := rand.Read(bytes)
	if err != nil {
		return 0, errors.Wrap(err, "read rand bytes")
	}

	result, err := typeutil.BytesToUint64(bytes)
	if err != nil {
		return 0, errors.Wrap(err, "convert bytes to int64")
	}

	return result, nil
}
