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

package etcdutil

import (
	"context"
	"testing"

	"github.com/stretchr/testify/require"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.etcd.io/etcd/server/v3/embed"
	"go.uber.org/goleak"

	"github.com/AutoMQ/placement-manager/pkg/util/testutil"
)

func TestMain(m *testing.M) {
	goleak.VerifyTestMain(m)
}

func TestGetOne(t *testing.T) {
	type args struct {
		key []byte
	}
	tests := []struct {
		name    string
		preset  map[string]string
		args    args
		want    []byte
		wantErr bool
		wantNil bool
	}{

		{
			name:   "get by single key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key: []byte("test/key1"),
			},
			want: []byte("val1"),
		},
		{
			name:   "query by nonexistent key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key: []byte("test/key0"),
			},
			wantNil: true,
		},
		{
			name:   "query by empty key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key: []byte(""),
			},
			wantErr: true,
		},
		{
			name:   "query by nil key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key: nil,
			},
			wantErr: true,
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)
			_, client, closeFunc := startEtcd(re, t)
			defer closeFunc()

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			got, err := GetOne(client, tt.args.key)

			// check
			if tt.wantErr {
				re.Error(err)
				return
			}
			re.NoError(err)
			if tt.wantNil {
				re.Nil(got)
				return
			}
			re.Equal(tt.args.key, got.Key)
			re.Equal(tt.want, got.Value)
		})
	}
}

func TestGet(t *testing.T) {
	type args struct {
		key  []byte
		opts []clientv3.OpOption
	}
	tests := []struct {
		name    string
		preset  map[string]string
		args    args
		want    map[string]string
		wantErr bool
	}{
		{
			name:   "get by single key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key:  []byte("test/key1"),
				opts: []clientv3.OpOption{},
			},
			want:    map[string]string{"test/key1": "val1"},
			wantErr: false,
		},
		{
			name:   "range query",
			preset: map[string]string{"test/key1": "val1", "test/key2": "val2", "test/key3": "val3", "test/key4": "val4"},
			args: args{
				key:  []byte("test/key2"),
				opts: []clientv3.OpOption{clientv3.WithRange("test/key4")},
			},
			want:    map[string]string{"test/key2": "val2", "test/key3": "val3"},
			wantErr: false,
		},
		{
			name:   "range query with limit",
			preset: map[string]string{"test/key1": "val1", "test/key2": "val2", "test/key3": "val3", "test/key4": "val4"},
			args: args{
				key:  []byte("test/key2"),
				opts: []clientv3.OpOption{clientv3.WithRange("test/key4"), clientv3.WithLimit(1), clientv3.WithSort(clientv3.SortByKey, clientv3.SortDescend)},
			},
			want:    map[string]string{"test/key3": "val3"},
			wantErr: false,
		},
		{
			name:   "query by prefix",
			preset: map[string]string{"test/key1": "val1", "test/key2": "val2", "test/key3": "val3", "another/key": "val"},
			args: args{
				key:  []byte("test/"),
				opts: []clientv3.OpOption{clientv3.WithRange(clientv3.GetPrefixRangeEnd("test/"))},
			},
			want:    map[string]string{"test/key1": "val1", "test/key2": "val2", "test/key3": "val3"},
			wantErr: false,
		},
		{
			name:   "query by nonexistent key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key:  []byte("test/key0"),
				opts: []clientv3.OpOption{},
			},
			want:    map[string]string{},
			wantErr: false,
		},
		{
			name:   "query by empty key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key:  []byte(""),
				opts: []clientv3.OpOption{},
			},
			wantErr: true,
		},
		{
			name:   "query by nil key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key:  nil,
				opts: []clientv3.OpOption{},
			},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)
			_, client, closeFunc := startEtcd(re, t)
			defer closeFunc()

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			resp, err := Get(client, tt.args.key, tt.args.opts...)

			// check
			if tt.wantErr {
				re.Error(err)
			} else {
				re.NoError(err)
				re.Len(resp.Kvs, len(tt.want))
				for _, keyValue := range resp.Kvs {
					v, ok := tt.want[string(keyValue.Key)]
					re.True(ok)
					re.Equal(v, string(keyValue.Value))
				}
			}
		})
	}
}

func TestPut(t *testing.T) {
	type args struct {
		key   []byte
		value []byte
		opts  []clientv3.OpOption
	}
	tests := []struct {
		name    string
		preset  map[string]string
		args    args
		want    map[string]string
		wantErr bool
	}{
		{
			name:   "put a new value",
			preset: map[string]string{},
			args: args{
				key:   []byte("test/key1"),
				value: []byte("val1"),
			},
			want: map[string]string{"test/key1": "val1"},
		},
		{
			name:   "put a new value with prevKV",
			preset: map[string]string{},
			args: args{
				key:   []byte("test/key1"),
				value: []byte("val1"),
				opts:  []clientv3.OpOption{clientv3.WithPrevKV()},
			},
			want: map[string]string{"test/key1": "val1"},
		},
		{
			name:   "put a value to an existing key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key:   []byte("test/key1"),
				value: []byte("val2"),
			},
			want: map[string]string{"test/key1": "val2"},
		},
		{
			name:   "put empty value",
			preset: map[string]string{},
			args: args{
				key:   []byte("test/key1"),
				value: []byte(""),
			},
			want: map[string]string{"test/key1": ""},
		},
		{
			name:   "put nil value",
			preset: map[string]string{},
			args: args{
				key:   []byte("test/key1"),
				value: nil,
			},
			want: map[string]string{"test/key1": ""},
		},
		{
			name:   "put empty key",
			preset: map[string]string{},
			args: args{
				key:   []byte(""),
				value: []byte("val1"),
			},
			wantErr: true,
		},
		{
			name:   "put nil key",
			preset: map[string]string{},
			args: args{
				key:   nil,
				value: []byte("val1"),
			},
			wantErr: true,
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)
			_, client, closeFunc := startEtcd(re, t)
			defer closeFunc()

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			_, err := Put(client, tt.args.key, tt.args.value, tt.args.opts...)

			// check
			if tt.wantErr {
				re.Error(err)
				return
			} else {
				re.NoError(err)
				for k, v := range tt.want {
					resp, err := kv.Get(context.Background(), k)
					re.NoError(err)
					re.Len(resp.Kvs, 1)
					re.Equal(v, string(resp.Kvs[0].Value))
				}
			}
		})
	}
}

func TestDelete(t *testing.T) {
	type args struct {
		key  []byte
		opts []clientv3.OpOption
	}
	tests := []struct {
		name     string
		preset   map[string]string
		args     args
		wantPrev map[string]string
		wantErr  bool
	}{
		{
			name:   "delete a key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key: []byte("test/key1"),
			},
			wantPrev: map[string]string{},
		},
		{
			name:   "delete a key with prevKV",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key:  []byte("test/key1"),
				opts: []clientv3.OpOption{clientv3.WithPrevKV()},
			},
			wantPrev: map[string]string{"test/key1": "val1"},
		},
		{
			name:   "delete a non-existing key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key: []byte("test/key2"),
			},
			wantPrev: map[string]string{},
		},
		{
			name:   "delete a non-existing key with prevKV",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key:  []byte("test/key2"),
				opts: []clientv3.OpOption{clientv3.WithPrevKV()},
			},
			wantPrev: map[string]string{},
		},
		{
			name:   "delete an empty key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key: []byte(""),
			},
			wantErr: true,
		},
		{
			name:   "delete a nil key",
			preset: map[string]string{"test/key1": "val1"},
			args: args{
				key: nil,
			},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)
			_, client, closeFunc := startEtcd(re, t)
			defer closeFunc()

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			resp, err := Delete(client, tt.args.key, tt.args.opts...)

			// check
			if tt.wantErr {
				re.Error(err)
				return
			} else {
				re.NoError(err)
				get, err := kv.Get(context.Background(), string(tt.args.key))
				re.NoError(err)
				re.Len(get.Kvs, 0)

				re.Len(resp.PrevKvs, len(tt.wantPrev))
				for _, kv := range resp.PrevKvs {
					re.Equal(tt.wantPrev[string(kv.Key)], string(kv.Value))
				}
			}
		})
	}
}

func startEtcd(re *require.Assertions, t *testing.T) (*embed.Etcd, *clientv3.Client, func()) {
	// start etcd
	cfg := testutil.NewEtcdConfig(t)
	etcd, err := embed.StartEtcd(cfg)
	re.NoError(err)

	// new client
	ep := cfg.LCUrls[0].String()
	client, err := clientv3.New(clientv3.Config{
		Endpoints: []string{ep},
	})
	re.NoError(err)

	<-etcd.Server.ReadyNotify()

	return etcd, client, func() { _ = client.Close(); etcd.Close() }
}
