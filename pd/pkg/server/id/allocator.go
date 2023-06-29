// Copyright 2016 TiKV Project Authors.
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

package id

import (
	"context"
)

const (
	_defaultStep  = 1000
	_defaultStart = 0
)

// Allocator is the allocator to generate unique ID.
type Allocator interface {
	// Alloc allocates a new ID.
	Alloc(ctx context.Context) (uint64, error)

	// AllocN allocates N continuous IDs.
	AllocN(ctx context.Context, n int) ([]uint64, error)

	// Reset resets the allocator to the start ID.
	Reset(ctx context.Context) error
}
