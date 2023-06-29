// Copyright 2022 TiKV Project Authors.
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

package endpoint

import (
	"go.uber.org/zap"

	"github.com/AutoMQ/pd/pkg/server/storage/kv"
)

const (
	_int64Format = "%020d" // max length of int64 is 20 (-9223372036854775807)
	_int64Len    = 20
	_int32Format = "%011d" // max length of int32 is 11 (-2147483647)
	_int32Len    = 11
)

const (
	// MinStreamID is the minimum stream ID.
	MinStreamID int64 = 0
	// MinRangeIndex is the minimum range index.
	MinRangeIndex int32 = 0
	// MinDataNodeID is the minimum data node ID.
	MinDataNodeID int32 = 0
)

// Endpoint is the base underlying storage endpoint for all other upper
// specific storage backends. It should define some common storage interfaces and operations,
// which provides the default implementations for all kinds of storages.
type Endpoint struct {
	kv.KV

	lg *zap.Logger
}

// NewEndpoint creates a new base storage endpoint with the given KV.
// It should be embedded inside a storage backend.
func NewEndpoint(kv2 kv.KV, logger *zap.Logger) *Endpoint {
	return &Endpoint{
		KV: kv2,
		lg: logger,
	}
}
