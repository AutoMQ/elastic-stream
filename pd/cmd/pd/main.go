// Package main is the entrypoint for pd.
package main

import (
	"context"
	"fmt"
	"os"
	"os/signal"
	"syscall"

	"github.com/pkg/errors"
	"github.com/spf13/pflag"
	"go.uber.org/zap"

	"github.com/AutoMQ/pd/pkg/server"
	"github.com/AutoMQ/pd/pkg/server/config"
)

func main() {
	cfg, err := config.NewConfig(os.Args[1:], os.Stderr)
	if errors.Is(err, pflag.ErrHelp) {
		os.Exit(0)
	}

	// create a logger first
	logger := cfg.Logger()
	if logger == nil {
		// something went wrong, create a new temporary logger
		var zapErr error
		logger, zapErr = zap.NewProduction()
		if zapErr != nil {
			fmt.Printf("error creating zap logger %v", zapErr)
			os.Exit(1)
		}
	}
	logger.Info("running", zap.Strings("args", os.Args))
	if err != nil {
		logger.Error("failed to parse config", zap.Error(err))
		os.Exit(1)
	}

	syncLogger := func() { _ = cfg.Logger().Sync() }

	// check config
	err = cfg.Adjust()
	if err != nil {
		logger.Error("failed to adjust config", zap.Error(err))
		exit(1, syncLogger)
	}
	err = cfg.Validate()
	if err != nil {
		logger.Error("failed to validate config", zap.Error(err))
		exit(1, syncLogger)
	}

	// create server
	ctx, cancel := context.WithCancel(context.Background())
	svr, err := server.NewServer(ctx, cfg, logger)
	if err != nil {
		logger.Error("failed to create server", zap.Error(err))
		exit(1, syncLogger)
	}

	// start server
	sc := make(chan os.Signal, 1)
	signal.Notify(sc,
		syscall.SIGHUP,
		syscall.SIGINT,
		syscall.SIGTERM,
		syscall.SIGQUIT)

	var sig os.Signal
	go func() {
		sig = <-sc
		cancel()
	}()

	err = svr.Start()
	if err != nil {
		logger.Error("failed to start server", zap.Error(err))
		exit(1, syncLogger)
	}

	// close server
	<-ctx.Done()
	logger.Info("got signal to exit", zap.String("signal", sig.String()))

	svr.Close()
	switch sig {
	case syscall.SIGTERM:
		exit(0, syncLogger)
	default:
		exit(1, syncLogger)
	}
}

func exit(code int, deferred func()) {
	deferred()
	os.Exit(code)
}
