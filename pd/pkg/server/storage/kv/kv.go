// Copyright 2017 TiKV Project Authors.
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

package kv

import (
	"context"
)

const (
	// KeySeparator is the separator in keys
	KeySeparator = "/"
)

// Range represents a range of keys.
type Range struct {
	StartKey []byte
	EndKey   []byte
}

// KeyValue represents a key-value pair.
type KeyValue struct {
	Key   []byte
	Value []byte
}

// KV represents the basic interface for a key-value store.
type KV interface {
	// Get retrieves the value associated with the given key.
	// If the key does not exist, Get returns nil and no error.
	// If the key is empty, Get returns nil and no error.
	Get(ctx context.Context, key []byte) ([]byte, error)

	// BatchGet retrieves the values associated with the given keys.
	// If the key does not exist, BatchGet returns no error.
	// Any empty key will be ignored.
	// If inTxn is true, BatchGet will try to get all keys in a single transaction.
	BatchGet(ctx context.Context, keys [][]byte, inTxn bool) ([]KeyValue, error)

	// GetByRange retrieves a list of key-value pairs whose keys fall within the given range (r)
	// and limits the number of results returned to "limit".
	// If the Range.StartKey is empty, GetByRange returns nil and no error.
	// If limit is 0, GetByRange will return all key-value pairs whose keys fall within the given range (r).
	// If limit is greater than 0, GetByRange will return at most "limit" key-value pairs whose keys fall within the given range (r),
	// and a boolean value indicating whether there are more keys in the range.
	// If desc is true, GetByRange returns the key-value pairs in descending order.
	GetByRange(ctx context.Context, r Range, limit int64, desc bool) (kvs []KeyValue, more bool, err error)

	// Put sets the value for the given key.
	// IF the key is empty, Put does nothing and returns no error.
	// If the key already exists, Put overwrites the existing value.
	// If prevKV is true, the old value (if any) will be returned.
	Put(ctx context.Context, key, value []byte, prevKV bool) ([]byte, error)

	// BatchPut sets the value for the given keys.
	// If the key already exists, BatchPut overwrites the existing value.
	// Any empty key will be ignored.
	// If prevKV is true, the old key-value pairs (if any) will be returned.
	// If inTxn is true, BatchPut will try to put all keys in a single transaction.
	BatchPut(ctx context.Context, kvs []KeyValue, prevKV bool, inTxn bool) ([]KeyValue, error)

	// Delete removes the key-value pair associated with the given key.
	// If the key is empty, Delete does nothing and returns no error.
	// If the key does not exist, Delete does nothing and returns no error.
	// If prevKV is true, the old value (if any) will be returned.
	Delete(ctx context.Context, key []byte, prevKV bool) ([]byte, error)

	// BatchDelete removes the key-value pairs associated with the given keys.
	// If the key does not exist, BatchDelete returns no error.
	// Any empty key will be ignored.
	// If prevKV is true, the old key-value pairs (if any) will be returned.
	// If inTxn is true, BatchDelete will try to delete all keys in a single transaction.
	BatchDelete(ctx context.Context, keys [][]byte, prevKV bool, inTxn bool) ([]KeyValue, error)

	// GetPrefixRangeEnd returns the end key for a prefix range query.
	GetPrefixRangeEnd(prefix []byte) []byte
}
