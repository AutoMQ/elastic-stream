package traceutil

import (
	"context"

	"go.uber.org/zap"
)

type traceIDKey struct{}

// SetTraceID sets the traceID into the context.
func SetTraceID(ctx context.Context, traceID string) context.Context {
	return context.WithValue(ctx, traceIDKey{}, traceID)
}

// TraceID returns the traceID from the context.
func TraceID(ctx context.Context) string {
	if traceID, ok := ctx.Value(traceIDKey{}).(string); ok {
		return traceID
	}
	return ""
}

// TraceLogField returns a zap.Field for logging.
// It returns zap.Skip() if the traceID is not found in the context.
func TraceLogField(ctx context.Context) zap.Field {
	if traceID, ok := ctx.Value(traceIDKey{}).(string); ok {
		return zap.String("trace-id", traceID)
	}
	return zap.Skip()
}
