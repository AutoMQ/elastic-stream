//go:build testing

// Copyright 2018 TiKV Project Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

package url

import (
	"fmt"
	"net"
	"sync"
	"testing"
	"time"
)

var (
	testAddrMutex sync.Mutex
	testAddrMap   = make(map[string]struct{})
)

func Alloc(tb testing.TB) string {
	return fmt.Sprintf("http://%s", AllocAddr(tb))
}

// AllocAddr allocates a local address (like host:port) for testing.
func AllocAddr(tb testing.TB) string {
	for i := 0; i < 10; i++ {
		if u := tryAllocTestAddr(tb); u != "" {
			return u
		}
		time.Sleep(time.Second)
	}
	tb.Fatal("failed to alloc test URL")
	return ""
}

func tryAllocTestAddr(tb testing.TB) string {
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		tb.Fatal("listen failed", err)
	}
	addr := l.Addr().String()
	err = l.Close()
	if err != nil {
		tb.Fatal("close failed", err)
	}

	testAddrMutex.Lock()
	defer testAddrMutex.Unlock()
	if _, ok := testAddrMap[addr]; ok {
		return ""
	}
	if !environmentCheck(addr, tb) {
		return ""
	}
	testAddrMap[addr] = struct{}{}
	return addr
}
