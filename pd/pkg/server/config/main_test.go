package config

import (
	"testing"

	"go.uber.org/goleak"
)

func TestMain(m *testing.M) {
	opt := goleak.IgnoreTopFunction("gopkg.in/natefinch/lumberjack%2ev2.(*Logger).millRun")
	goleak.VerifyTestMain(m, opt)
}
