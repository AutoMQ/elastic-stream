package traceutil

import (
	"context"
	"testing"

	"github.com/stretchr/testify/require"
)

func TestTraceID(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	ctx := context.Background()
	re.Equal("", TraceID(ctx))

	tCtx1 := SetTraceID(ctx, "test1")
	re.Equal("test1", TraceID(tCtx1))

	tCtx2 := SetTraceID(ctx, "")
	re.Equal("", TraceID(tCtx2))

	tCtx3 := SetTraceID(ctx, "test3")
	re.Equal("test3", TraceID(tCtx3))
}
