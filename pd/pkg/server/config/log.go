package config

import (
	"fmt"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/pkg/errors"
	"github.com/spf13/pflag"
	"github.com/spf13/viper"
	"go.uber.org/zap"
	"go.uber.org/zap/buffer"
	"go.uber.org/zap/zapcore"
	"gopkg.in/natefinch/lumberjack.v2"
)

const (
	RotationSchema = "rotate" // RotationSchema is used to identify the log files that need to be rotated

	_callerDepth = 2 // callerDepth is used to get the caller of the logging function
)

var (
	_bufPool = buffer.NewPool()
)

// Log is configuration item for logging, including configuration for Zap.Logger and log rotation
type Log struct {
	Zap            zap.Config
	Rotate         Rotate
	EnableRotation bool
	Level          string
}

// NewLog creates a default logging configuration.
func NewLog() *Log {
	log := &Log{
		Zap: zap.NewProductionConfig(),
	}
	log.Zap.EncoderConfig.EncodeCaller = encodeCaller
	log.Zap.EncoderConfig.EncodeTime = zapcore.ISO8601TimeEncoder
	log.Zap.EncoderConfig.EncodeDuration = DurationEncoder
	return log
}

func DurationEncoder(d time.Duration, enc zapcore.PrimitiveArrayEncoder) {
	switch {
	case d < time.Microsecond:
		enc.AppendString(fmt.Sprintf("%dns", d.Nanoseconds()))
	case d < time.Millisecond:
		enc.AppendString(fmt.Sprintf("%dus", d.Microseconds()))
	case d < time.Second:
		enc.AppendString(fmt.Sprintf("%dms", d.Milliseconds()))
	default:
		enc.AppendString(fmt.Sprintf("%.3fs", d.Seconds()))
	}
}

// Adjust adjusts the configuration in Log.Zap based on additional settings
func (l *Log) Adjust() error {
	if l.Zap.ErrorOutputPaths == nil {
		l.Zap.ErrorOutputPaths = make([]string, len(l.Zap.OutputPaths))
		copy(l.Zap.ErrorOutputPaths, l.Zap.OutputPaths)
	}

	if l.EnableRotation {
		wd, err := os.Getwd()
		if err != nil {
			return errors.Wrap(err, "get current directory")
		}
		l.Zap.OutputPaths = addRotationSchema(l.Zap.OutputPaths, wd)
		l.Zap.ErrorOutputPaths = addRotationSchema(l.Zap.ErrorOutputPaths, wd)
	}

	level, err := zapcore.ParseLevel(l.Level)
	if err != nil {
		return errors.Wrap(err, "parse log level")
	}
	l.Zap.Level = zap.NewAtomicLevelAt(level)

	return nil
}

// Logger creates a logger based on the configuration
func (l *Log) Logger() (*zap.Logger, error) {
	if l.EnableRotation {
		err := l.setupRotation()
		if err != nil {
			return nil, errors.Wrap(err, "setup rotation")
		}
	}

	logger, err := l.Zap.Build()
	if err != nil {
		return nil, errors.Wrap(err, "build logger")
	}
	return logger, nil
}

func logConfigure(v *viper.Viper, fs *pflag.FlagSet) {
	fs.String("log-level", _defaultLogLevel, "the minimum enabled logging level")
	fs.StringSlice("log-zap-output-paths", _defaultLogZapOutputPaths, "a list of URLs or file paths to write logging output to")
	fs.StringSlice("log-zap-error-output-paths", []string{}, "a list of URLs to write internal logger errors to (default ${log-zap-output-paths})")
	fs.String("log-zap-encoding", _defaultLogZapEncoding, "the logger's encoding, \"json\" or \"console\"")
	fs.Bool("log-enable-rotation", _defaultLogEnableRotation, "whether to enable log rotation")
	fs.Int("log-rotate-max-size", _defaultLogRotateMaxSize, "maximum size in megabytes of the log file before it gets rotated")
	fs.Int("log-rotate-max-age", _defaultLogRotateMaxAge, "maximum number of days to retain old log files based on the timestamp encoded in their filename")
	fs.Int("log-rotate-max-backups", _defaultLogRotateMaxBackups, "maximum number of old log files to retain, default is to retain all old log files (though MaxAge may still cause them to get deleted)")
	fs.Bool("log-rotate-local-time", _defaultLogRotateLocalTime, "whether the time used for formatting the timestamps in backup files is the computer's local time, default is to use UTC time")
	fs.Bool("log-rotate-compress", _defaultLogRotateCompress, "whether the rotated log files should be compressed using gzip")
	_ = v.BindPFlag("log.level", fs.Lookup("log-level"))
	_ = v.BindPFlag("log.zap.outputPaths", fs.Lookup("log-zap-output-paths"))
	_ = v.BindPFlag("log.zap.errorOutputPaths", fs.Lookup("log-zap-error-output-paths"))
	_ = v.BindPFlag("log.zap.encoding", fs.Lookup("log-zap-encoding"))
	_ = v.BindPFlag("log.enableRotation", fs.Lookup("log-enable-rotation"))
	_ = v.BindPFlag("log.rotate.maxSize", fs.Lookup("log-rotate-max-size"))
	_ = v.BindPFlag("log.rotate.maxAge", fs.Lookup("log-rotate-max-age"))
	_ = v.BindPFlag("log.rotate.maxBackups", fs.Lookup("log-rotate-max-backups"))
	_ = v.BindPFlag("log.rotate.localTime", fs.Lookup("log-rotate-local-time"))
	_ = v.BindPFlag("log.rotate.compress", fs.Lookup("log-rotate-compress"))
}

func encodeCaller(caller zapcore.EntryCaller, enc zapcore.PrimitiveArrayEncoder) {
	if !caller.Defined {
		enc.AppendString("<unknown>")
		return
	}

	idx := indexByteBackward(caller.File, '/', _callerDepth+1)
	if idx == -1 {
		enc.AppendString(caller.FullPath())
		return
	}

	buf := _bufPool.Get()
	defer buf.Free()
	buf.AppendString(caller.File[idx+1:])
	buf.AppendByte(':')
	buf.AppendInt(int64(caller.Line))
	enc.AppendString(buf.String())
}

func indexByteBackward(s string, c byte, cnt int) int {
	idx := len(s)
	for cnt > 0 && idx != -1 {
		idx = strings.LastIndexByte(s[:idx], c)
		cnt--
	}
	return idx
}

// Rotate is a copy of the configuration section in lumberjack.Logger
type Rotate struct {
	// MaxSize is the maximum size in megabytes of the log file before it gets
	// rotated. It defaults to 100 megabytes.
	MaxSize int

	// MaxAge is the maximum number of days to retain old log files based on the
	// timestamp encoded in their filename.  Note that a day is defined as 24
	// hours and may not exactly correspond to calendar days due to daylight
	// savings, leap seconds, etc. The default is not to remove old log files
	// based on age.
	MaxAge int

	// MaxBackups is the maximum number of old log files to retain.  The default
	// is to retain all old log files (though MaxAge may still cause them to get
	// deleted.)
	MaxBackups int

	// LocalTime determines if the time used for formatting the timestamps in
	// backup files is the computer's local time.  The default is to use UTC
	// time.
	LocalTime bool

	// Compress determines if the rotated log files should be compressed
	// using gzip. The default is not to perform compression.
	Compress bool
}

type rotation struct {
	lumberjack.Logger
}

// Sync implements zap.Sink. The remaining methods are implemented
// by the embedded *lumberjack.Logger.
func (*rotation) Sync() error {
	return nil
}

// setupRotation can only be called ONCE since a fixed schema is being used
func (l *Log) setupRotation() error {
	err := zap.RegisterSink(RotationSchema, func(url *url.URL) (zap.Sink, error) {
		return &rotation{lumberjack.Logger{
			Filename:   url.Path,
			MaxSize:    l.Rotate.MaxSize,
			MaxAge:     l.Rotate.MaxAge,
			MaxBackups: l.Rotate.MaxBackups,
			LocalTime:  l.Rotate.LocalTime,
			Compress:   l.Rotate.Compress,
		}}, nil
	})
	if err != nil {
		return errors.Wrap(err, "register sink")
	}
	return nil
}

func addRotationSchema(paths []string, wd string) []string {
	results := make([]string, len(paths))
	for i, path := range paths {
		switch path {
		case "stderr", "stdout":
			results[i] = path
		default:
			// add schema for file paths
			if !filepath.IsAbs(path) {
				path = filepath.Join(wd, path)
			}
			results[i] = fmt.Sprintf("%s:%s", RotationSchema, path)
		}
	}
	return results
}
