package randutil

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestUint64(t *testing.T) {
	re := require.New(t)
	result, err := Uint64()
	re.NoError(err)
	re.NotZero(result)
}
