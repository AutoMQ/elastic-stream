package config

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"
	"go.uber.org/zap"
	"go.uber.org/zap/zapcore"
)

func TestLog_Adjust(t *testing.T) {
	wd, err := os.Getwd()
	require.NoError(t, err)
	tests := []struct {
		name    string
		in      *Log
		want    *Log
		wantErr bool
		errMsg  string
	}{
		{
			name: "default config",
			in:   NewLog(),
			want: &Log{
				Zap:            zap.NewProductionConfig(),
				Rotate:         Rotate{},
				EnableRotation: false,
				Level:          "",
			},
		},
		{
			name: "normal config",
			in: func() *Log {
				l := NewLog()
				l.Zap.OutputPaths = []string{"test-output-path1", "/test-output-path2", "stderr", "stdout"}
				l.Zap.ErrorOutputPaths = nil
				l.EnableRotation = true
				l.Level = "DEBUG"
				return l
			}(),
			want: &Log{
				Zap: func() zap.Config {
					z := zap.NewProductionConfig()
					z.OutputPaths = []string{"rotate:" + filepath.Join(wd, "/test-output-path1"), "rotate:/test-output-path2", "stderr", "stdout"}
					z.ErrorOutputPaths = []string{"rotate:" + filepath.Join(wd, "/test-output-path1"), "rotate:/test-output-path2", "stderr", "stdout"}
					z.Level = zap.NewAtomicLevelAt(zapcore.DebugLevel)
					return z
				}(),
				Rotate:         Rotate{},
				EnableRotation: true,
				Level:          "DEBUG",
			},
		},
		{
			name: "invalid log level",
			in: func() *Log {
				l := NewLog()
				l.Level = "BAD"
				return l
			}(),
			wantErr: true,
			errMsg:  "parse log level",
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			err := tt.in.Adjust()

			if tt.wantErr {
				re.ErrorContains(err, tt.errMsg)
				return
			}
			re.NoError(err)

			equal(re, tt.want.Zap, tt.in.Zap)
			tt.want.Zap = zap.Config{}
			tt.in.Zap = zap.Config{}

			re.Equal(tt.want, tt.in)
		})
	}
}

func TestLogRotation(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	tempDir := t.TempDir()

	l := NewLog()
	l.EnableRotation = true
	l.Rotate.MaxSize = 1
	l.Rotate.MaxBackups = 3
	l.Zap.OutputPaths = []string{filepath.Join(tempDir, "test1", "pm.log")}

	err := l.Adjust()
	re.NoError(err)
	logger, err := l.Logger()
	re.NoError(err)

	msg := string(make([]byte, 1<<12))
	for i := 0; i < 4096; i++ {
		logger.Info(msg)
	}

	entries, err := os.ReadDir(filepath.Join(tempDir, "test1"))
	re.NoError(err)
	re.Len(entries, 4)
	for _, entry := range entries {
		info, err := entry.Info()
		re.NoError(err)
		re.LessOrEqual(info.Size(), int64(1<<20))
	}

	// test rotation error
	_, err = l.Logger()
	re.ErrorContains(err, "setup rotation")
	re.ErrorContains(err, "register sink")
}

func Test_indexByteBackward(t *testing.T) {
	type args struct {
		s   string
		c   byte
		cnt int
	}
	tests := []struct {
		name string
		args args
		want int
	}{
		{
			name: "normal",
			args: args{
				s:   "a/b/c/d/e.go",
				c:   '/',
				cnt: 2,
			},
			want: 5,
		},
		{
			name: "not found",
			args: args{
				s:   "a/b/c/d/e.go",
				c:   '/',
				cnt: 10,
			},
			want: -1,
		},
		{
			name: "cnt is 0",
			args: args{
				s:   "a/b/c/d/e.go",
				c:   '/',
				cnt: 0,
			},
			want: 12,
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			got := indexByteBackward(tt.args.s, tt.args.c, tt.args.cnt)
			re.Equal(tt.want, got)
		})
	}
}
