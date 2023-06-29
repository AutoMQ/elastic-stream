//go:build testing

package server

import (
	"time"
)

func init() {
	goAwayTimeout = 25 * time.Millisecond
}
