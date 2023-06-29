package id

import (
	"context"

	"go.uber.org/zap"

	"github.com/AutoMQ/pd/pkg/util/traceutil"
)

type LogAble interface {
	Allocator
	Logger() *zap.Logger
}

// Logger is a wrapper of Allocator that logs all operations.
type Logger struct {
	Allocator LogAble
}

func (l Logger) Alloc(ctx context.Context) (id uint64, err error) {
	id, err = l.Allocator.Alloc(ctx)

	logger := l.logger()
	if logger.Core().Enabled(zap.DebugLevel) {
		logger = logger.With(traceutil.TraceLogField(ctx))
		logger.Debug("alloc id", zap.Uint64("id", id), zap.Error(err))
	}
	return
}

func (l Logger) AllocN(ctx context.Context, n int) (ids []uint64, err error) {
	ids, err = l.Allocator.AllocN(ctx, n)

	logger := l.logger()
	if logger.Core().Enabled(zap.DebugLevel) {
		logger = logger.With(traceutil.TraceLogField(ctx))
		logger.Debug("alloc ids", zap.Int("num", n), zap.Uint64s("ids", ids), zap.Error(err))
	}
	return
}

func (l Logger) Reset(ctx context.Context) (err error) {
	err = l.Allocator.Reset(ctx)

	logger := l.logger()
	if logger.Core().Enabled(zap.DebugLevel) {
		logger = logger.With(traceutil.TraceLogField(ctx))
		logger.Debug("reset allocator", zap.Error(err))
	}
	return
}

func (l Logger) logger() *zap.Logger {
	if l.Allocator.Logger() != nil {
		return l.Allocator.Logger()
	}
	return zap.NewNop()
}
