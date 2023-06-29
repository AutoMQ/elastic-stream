package etcdutil

import (
	"context"
	"time"

	"github.com/pkg/errors"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.uber.org/zap"
)

// Txn wraps etcd transaction with a default timeout, and logs slow ones.
type Txn struct {
	clientv3.Txn
	cancel context.CancelFunc
	lg     *zap.Logger
}

// NewTxn create a Txn.
func NewTxn(ctx context.Context, kv clientv3.KV, lg *zap.Logger) clientv3.Txn {
	tCtx, cancel := context.WithTimeout(ctx, DefaultRequestTimeout)
	return &Txn{
		Txn:    kv.Txn(tCtx),
		cancel: cancel,
		lg:     lg,
	}
}

// If takes a list of comparison. If all comparisons passed in succeed,
// the operations passed into Then() will be executed. Or the operations
// passed into Else() will be executed.
func (t *Txn) If(cs ...clientv3.Cmp) clientv3.Txn {
	t.Txn = t.Txn.If(cs...)
	return t
}

// Then takes a list of operations. The Ops list will be executed, if the
// comparisons passed in If() succeed.
func (t *Txn) Then(ops ...clientv3.Op) clientv3.Txn {
	t.Txn = t.Txn.Then(ops...)
	return t
}

// Commit implements Txn Commit interface.
func (t *Txn) Commit() (*clientv3.TxnResponse, error) {
	start := time.Now()

	resp, err := t.Txn.Commit()
	t.cancel()

	logger := t.lg
	cost := time.Since(start)
	if cost > DefaultSlowRequestTime {
		logger.Warn("txn runs too slow", zap.Reflect("response", resp), zap.Duration("cost", cost), zap.Error(err))
	}
	// TODO add prometheus counters here

	return resp, errors.Wrap(err, "commit txn")
}
