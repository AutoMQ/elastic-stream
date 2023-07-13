package kv

import (
	"context"
	"testing"

	"github.com/stretchr/testify/require"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.etcd.io/etcd/server/v3/embed"
	"go.uber.org/zap"

	"github.com/AutoMQ/pd/pkg/util/testutil"
)

func TestEtcd_Get(t *testing.T) {
	type args struct {
		key []byte
	}
	tests := []struct {
		name    string
		preset  map[string]string
		args    args
		want    []byte
		wantErr bool
	}{
		{
			name: "get existing key",
			preset: map[string]string{
				"/test/key1": "val1",
			},
			args: args{
				key: []byte("key1"),
			},
			want: []byte("val1"),
		},
		{
			name: "get non-existing key",
			preset: map[string]string{
				"/test/key1": "val1",
			},
			args: args{
				key: []byte("key2"),
			},
			want: nil,
		},
		{
			name: "get empty key",
			preset: map[string]string{
				"/test/":     "val",
				"/test/key1": "val1",
			},
			args: args{
				key: []byte(""),
			},
			want: nil,
		},
		{
			name: "get nil key",
			preset: map[string]string{
				"/test/":     "val",
				"/test/key1": "val1",
			},
			args: args{
				key: nil,
			},
			want: nil,
		},
		{
			name: "get empty value",
			preset: map[string]string{
				"/test/key": "",
			},
			args: args{
				key: []byte("key"),
			},
			want: nil,
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)
			_, client, closeFunc := testutil.StartEtcd(t, nil)
			defer closeFunc()

			etcd := NewEtcd(EtcdParam{
				KV:       client,
				RootPath: "/test",
			}, zap.NewNop())

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			got, err := etcd.Get(context.Background(), tt.args.key)

			// check
			if tt.wantErr {
				re.Error(err)
			} else {
				re.NoError(err)
				re.Equal(tt.want, got)
			}
		})
	}
}

func TestEtcd_BatchGet(t *testing.T) {
	type fields struct {
		newCmpFunc func() clientv3.Cmp
		maxTxnOps  uint
	}
	type args struct {
		keys  [][]byte
		inTxn bool
	}
	tests := []struct {
		name    string
		preset  map[string]string
		fields  fields
		args    args
		want    []KeyValue
		wantErr bool
		errMsg  string
	}{
		{
			name: "get keys",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			args: args{keys: [][]byte{
				[]byte("key1"),
				[]byte("key2"),
			}},
			want: []KeyValue{
				{Key: []byte("key1"), Value: []byte("val1")},
				{Key: []byte("key2"), Value: []byte("val2")},
			},
		},
		{
			name: "get nonexistent key and existing key and empty key",
			preset: map[string]string{
				"/test/":     "val",
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			args: args{keys: [][]byte{
				[]byte("key0"),
				[]byte("key1"),
				[]byte(""),
			}},
			want: []KeyValue{
				{Key: []byte("key1"), Value: []byte("val1")},
			},
		},
		{
			name: "get with empty list",
			args: args{keys: [][]byte{}},
			want: nil,
		},
		{
			name: "get when transaction failed",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			fields: fields{
				newCmpFunc: alwaysFailedTxnFunc,
			},
			args: args{keys: [][]byte{
				[]byte("key0"),
				[]byte("key1"),
			}},
			wantErr: true,
			errMsg:  "etcd transaction failed",
		},
		{
			name: "get in multiple transactions",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			fields: fields{
				maxTxnOps: 1,
			},
			args: args{keys: [][]byte{
				[]byte("key1"),
				[]byte("key2"),
			}},
			want: []KeyValue{
				{Key: []byte("key1"), Value: []byte("val1")},
				{Key: []byte("key2"), Value: []byte("val2")},
			},
		},
		{
			name: "get when exceeding maxTxnOps",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			fields: fields{
				maxTxnOps: 1,
			},
			args: args{keys: [][]byte{
				[]byte("key1"),
				[]byte("key2"),
			}, inTxn: true},
			wantErr: true,
			errMsg:  "too many txn operations",
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			var cfg *embed.Config
			if tt.fields.maxTxnOps != 0 {
				cfg = testutil.NewEtcdConfig(t)
				cfg.MaxTxnOps = tt.fields.maxTxnOps
			}
			_, client, closeFunc := testutil.StartEtcd(t, cfg)
			defer closeFunc()

			etcd := NewEtcd(EtcdParam{
				KV:        client,
				RootPath:  "/test",
				CmpFunc:   tt.fields.newCmpFunc,
				MaxTxnOps: tt.fields.maxTxnOps,
			}, zap.NewNop())

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			got, err := etcd.BatchGet(context.Background(), tt.args.keys, tt.args.inTxn)

			// check
			if tt.wantErr {
				re.ErrorContains(err, tt.errMsg)
			} else {
				re.NoError(err)
				re.Equal(tt.want, got)
			}
		})
	}
}

func TestEtcd_GetByRange(t *testing.T) {
	type fields struct {
		newCmpFunc func() clientv3.Cmp
	}
	type args struct {
		r     Range
		limit int64
		desc  bool
	}
	type want struct {
		kvs  []KeyValue
		more bool
	}
	tests := []struct {
		name    string
		preset  map[string]string
		fields  fields
		args    args
		want    want
		wantErr bool
		errMsg  string
	}{
		{
			name:   "get keys",
			preset: map[string]string{"/test/key1": "val1", "/test/key2": "val2", "/test/key3": "val3"},
			args: args{
				r: Range{[]byte("key1"), []byte("key3")},
			},
			want: want{kvs: []KeyValue{{Key: []byte("key1"), Value: []byte("val1")}, {Key: []byte("key2"), Value: []byte("val2")}}},
		},
		{
			name:   "get keys with prefix",
			preset: map[string]string{"/test/key1": "val1", "/test/key2": "val2", "/test/key3": "val3"},
			args: args{
				r: Range{[]byte("key"), []byte(clientv3.GetPrefixRangeEnd("key"))},
			},
			want: want{kvs: []KeyValue{{Key: []byte("key1"), Value: []byte("val1")}, {Key: []byte("key2"), Value: []byte("val2")}, {Key: []byte("key3"), Value: []byte("val3")}}},
		},
		{
			name:   "get keys with limit",
			preset: map[string]string{"/test/key1": "val1", "/test/key2": "val2", "/test/key3": "val3"},
			args: args{
				r:     Range{[]byte("key1"), []byte("key3")},
				limit: 1,
			},
			want: want{kvs: []KeyValue{{Key: []byte("key1"), Value: []byte("val1")}}, more: true},
		},
		{
			name:   "get keys with desc",
			preset: map[string]string{"/test/key1": "val1", "/test/key2": "val2", "/test/key3": "val3"},
			args: args{
				r:    Range{[]byte("key1"), []byte("key3")},
				desc: true,
			},
			want: want{kvs: []KeyValue{{Key: []byte("key2"), Value: []byte("val2")}, {Key: []byte("key1"), Value: []byte("val1")}}},
		},

		{
			name:   "end key greater than start key",
			preset: map[string]string{"/test/key1": "val1", "/test/key2": "val2", "/test/key3": "val3"},
			args: args{
				r: Range{[]byte("key3"), []byte("key1")},
			},
			want: want{kvs: []KeyValue{}},
		},
		{
			name:   "get keys with empty range",
			preset: map[string]string{"/test/key1": "val1", "/test/key2": "val2", "/test/key3": "val3"},
			args: args{
				r: Range{[]byte(""), []byte("")},
			},
			want: want{},
		},
		{
			name:   "get when transaction failed",
			preset: map[string]string{"/test/key1": "val1", "/test/key2": "val2", "/test/key3": "val3"},
			fields: fields{newCmpFunc: alwaysFailedTxnFunc},
			args: args{
				r: Range{[]byte("key1"), []byte("key3")},
			},
			wantErr: true,
			errMsg:  "etcd transaction failed",
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)
			_, client, closeFunc := testutil.StartEtcd(t, nil)
			defer closeFunc()

			etcd := NewEtcd(EtcdParam{
				KV:       client,
				RootPath: "/test",
				CmpFunc:  tt.fields.newCmpFunc,
			}, zap.NewNop())

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			kvs, more, err := etcd.GetByRange(context.Background(), tt.args.r, tt.args.limit, tt.args.desc)

			// check
			if tt.wantErr {
				re.ErrorContains(err, tt.errMsg)
			} else {
				re.NoError(err)
				re.Equal(tt.want, want{kvs: kvs, more: more})
			}
		})
	}
}

func TestEtcd_Put(t *testing.T) {
	type args struct {
		key    []byte
		value  []byte
		prevKV bool
	}
	tests := []struct {
		name    string
		preset  map[string]string
		args    args
		want    []byte
		after   map[string]string
		wantErr bool
	}{
		{
			name: "put new key",
			preset: map[string]string{
				"/test/key0": "val0",
			},
			args: args{
				key:    []byte("key1"),
				value:  []byte("val1"),
				prevKV: true,
			},
			want: nil,
			after: map[string]string{
				"/test/key0": "val0",
				"/test/key1": "val1",
			},
		},
		{
			name: "put existing key",
			preset: map[string]string{
				"/test/key1": "val1",
			},
			args: args{
				key:    []byte("key1"),
				value:  []byte("val2"),
				prevKV: true,
			},
			want: []byte("val1"),
			after: map[string]string{
				"/test/key1": "val2",
			},
		},
		{
			name: "put existing key without prevKV",
			preset: map[string]string{
				"/test/key1": "val1",
			},
			args: args{
				key:   []byte("key1"),
				value: []byte("val2"),
			},
			want: nil,
			after: map[string]string{
				"/test/key1": "val2",
			},
		},
		{
			name: "put with empty key",
			preset: map[string]string{
				"/test/":     "val",
				"/test/key1": "val1",
			},
			args: args{
				key:    []byte(""),
				value:  []byte("val2"),
				prevKV: true,
			},
			want: nil,
			after: map[string]string{
				"/test/":     "val",
				"/test/key1": "val1",
			},
		},
		{
			name: "put with nil key",
			preset: map[string]string{
				"/test/":     "val",
				"/test/key1": "val1",
			},
			args: args{
				key:    nil,
				value:  []byte("val2"),
				prevKV: true,
			},
			want: nil,
			after: map[string]string{
				"/test/":     "val",
				"/test/key1": "val1",
			},
		},
		{
			name: "put empty value",
			preset: map[string]string{
				"/test/key1": "val1",
			},
			args: args{
				key:    []byte("key1"),
				value:  []byte(""),
				prevKV: true,
			},
			want: []byte("val1"),
			after: map[string]string{
				"/test/key1": "",
			},
		},
		{
			name: "put nil value",
			preset: map[string]string{
				"/test/key1": "val1",
			},
			args: args{
				key:    []byte("key1"),
				value:  nil,
				prevKV: true,
			},
			want: []byte("val1"),
			after: map[string]string{
				"/test/key1": "",
			},
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)
			_, client, closeFunc := testutil.StartEtcd(t, nil)
			defer closeFunc()

			etcd := NewEtcd(EtcdParam{
				KV:       client,
				RootPath: "/test",
			}, zap.NewNop())

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			got, err := etcd.Put(context.Background(), tt.args.key, tt.args.value, tt.args.prevKV)

			// check
			if tt.wantErr {
				re.Error(err)
			} else {
				re.NoError(err)
				re.Equal(tt.want, got)
			}
			resp, err := kv.Get(context.Background(), "/test", clientv3.WithPrefix())
			re.NoError(err)
			re.Equal(len(tt.after), len(resp.Kvs))
			for _, kvs := range resp.Kvs {
				re.Equal(tt.after[string(kvs.Key)], string(kvs.Value))
			}
		})
	}
}

func TestEtcd_BatchPut(t *testing.T) {
	type fields struct {
		newCmpFunc func() clientv3.Cmp
		maxTxnOps  uint
	}
	type args struct {
		kvs    []KeyValue
		prevKV bool
		inTxn  bool
	}
	tests := []struct {
		name    string
		preset  map[string]string
		fields  fields
		args    args
		want    []KeyValue
		after   map[string]string
		wantErr bool
		errMsg  string
	}{
		{
			name: "put new key",
			preset: map[string]string{
				"/test/key0": "val0",
			},
			args: args{
				kvs: []KeyValue{
					{
						Key:   []byte("key1"),
						Value: []byte("val1"),
					},
					{
						Key:   []byte("key2"),
						Value: []byte("val2"),
					},
				},
				prevKV: true,
			},
			want: []KeyValue{},
			after: map[string]string{
				"/test/key0": "val0",
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
		},
		{
			name: "put existing key",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			args: args{
				kvs: []KeyValue{
					{
						Key:   []byte("key1"),
						Value: []byte("val10"),
					},
					{
						Key:   []byte("key2"),
						Value: []byte("val20"),
					},
				},
				prevKV: true,
			},
			want: []KeyValue{
				{
					Key:   []byte("key1"),
					Value: []byte("val1"),
				},
				{
					Key:   []byte("key2"),
					Value: []byte("val2"),
				},
			},
			after: map[string]string{
				"/test/key1": "val10",
				"/test/key2": "val20",
			},
		},
		{
			name: "put existing key without prevKV",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			args: args{
				kvs: []KeyValue{
					{
						Key:   []byte("key1"),
						Value: []byte("val10"),
					},
					{
						Key:   []byte("key2"),
						Value: []byte("val20"),
					},
				},
			},
			want: nil,
			after: map[string]string{
				"/test/key1": "val10",
				"/test/key2": "val20",
			},
		},
		{
			name: "put new key, existing key, empty key, nil key, empty value, nil value",
			preset: map[string]string{
				"/test/":     "val",
				"/test/key0": "val0",
				"/test/key1": "val1",
				"/test/key3": "val3",
				"/test/key4": "val4",
			},
			args: args{
				kvs: []KeyValue{
					{
						Key:   []byte("key1"),
						Value: []byte("val10"),
					},
					{
						Key:   []byte("key2"),
						Value: []byte("val2"),
					},
					{
						Key:   []byte(""),
						Value: []byte("v"),
					},
					{
						Key:   nil,
						Value: []byte("v"),
					},
					{
						Key:   []byte("key3"),
						Value: []byte(""),
					},
					{
						Key:   []byte("key4"),
						Value: nil,
					},
				},
				prevKV: true,
			},
			want: []KeyValue{
				{
					Key:   []byte("key1"),
					Value: []byte("val1"),
				},
				{
					Key:   []byte("key3"),
					Value: []byte("val3"),
				},
				{
					Key:   []byte("key4"),
					Value: []byte("val4"),
				},
			},
			after: map[string]string{
				"/test/":     "val",
				"/test/key0": "val0",
				"/test/key1": "val10",
				"/test/key2": "val2",
				"/test/key3": "",
				"/test/key4": "",
			},
		},
		{
			name: "put with empty list",
			preset: map[string]string{
				"/test/key0": "val0",
			},
			args: args{
				kvs: []KeyValue{},
			},
			want: nil,
			after: map[string]string{
				"/test/key0": "val0",
			},
		},
		{
			name: "put when transaction failed",
			preset: map[string]string{
				"/test/key0": "val0",
			},
			fields: fields{
				newCmpFunc: alwaysFailedTxnFunc,
			},
			args: args{
				kvs: []KeyValue{
					{
						Key:   []byte("key1"),
						Value: []byte("val1"),
					},
				},
			},
			after: map[string]string{
				"/test/key0": "val0",
			},
			wantErr: true,
			errMsg:  "etcd transaction failed",
		},
		{
			name: "put in multiple transactions",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			fields: fields{
				maxTxnOps: 1,
			},
			args: args{
				kvs: []KeyValue{
					{
						Key:   []byte("key1"),
						Value: []byte("val10"),
					},
					{
						Key:   []byte("key2"),
						Value: []byte("val20"),
					},
				},
				prevKV: true,
			},
			want: []KeyValue{
				{
					Key:   []byte("key1"),
					Value: []byte("val1"),
				},
				{
					Key:   []byte("key2"),
					Value: []byte("val2"),
				},
			},
			after: map[string]string{
				"/test/key1": "val10",
				"/test/key2": "val20",
			},
		},
		{
			name: "put when exceeding maxTxnOps",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			fields: fields{
				maxTxnOps: 1,
			},
			args: args{
				kvs: []KeyValue{
					{
						Key:   []byte("key1"),
						Value: []byte("val10"),
					},
					{
						Key:   []byte("key2"),
						Value: []byte("val20"),
					},
				},
				prevKV: true,
				inTxn:  true,
			},
			wantErr: true,
			errMsg:  "too many txn operations",
			after: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			var cfg *embed.Config
			if tt.fields.maxTxnOps != 0 {
				cfg = testutil.NewEtcdConfig(t)
				cfg.MaxTxnOps = tt.fields.maxTxnOps
			}
			_, client, closeFunc := testutil.StartEtcd(t, cfg)
			defer closeFunc()

			etcd := NewEtcd(EtcdParam{
				KV:        client,
				RootPath:  "/test",
				CmpFunc:   tt.fields.newCmpFunc,
				MaxTxnOps: tt.fields.maxTxnOps,
			}, zap.NewNop())

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			got, err := etcd.BatchPut(context.Background(), tt.args.kvs, tt.args.prevKV, tt.args.inTxn)

			// check
			if tt.wantErr {
				re.ErrorContains(err, tt.errMsg)
			} else {
				re.NoError(err)
				re.Equal(tt.want, got)
			}
			resp, err := kv.Get(context.Background(), "/test", clientv3.WithPrefix())
			re.NoError(err)
			re.Equal(len(tt.after), len(resp.Kvs))
			for _, kvs := range resp.Kvs {
				re.Equal(tt.after[string(kvs.Key)], string(kvs.Value))
			}
		})
	}
}

func TestEtcd_Delete(t *testing.T) {
	type args struct {
		key    []byte
		prevKV bool
	}
	tests := []struct {
		name    string
		preset  map[string]string
		args    args
		want    []byte
		after   map[string]string
		wantErr bool
	}{
		{
			name: "delete existing key",
			preset: map[string]string{
				"/test/key1": "val1",
			},
			args: args{
				key: []byte("key1"),
			},
			want:  nil,
			after: map[string]string{},
		},
		{
			name: "delete existing key and get prevKV",
			preset: map[string]string{
				"/test/key1": "val1",
			},
			args: args{
				key:    []byte("key1"),
				prevKV: true,
			},
			want:  []byte("val1"),
			after: map[string]string{},
		},
		{
			name: "delete nonexistent key",
			preset: map[string]string{
				"/test/key1": "val1",
			},
			args: args{
				key:    []byte("key0"),
				prevKV: true,
			},
			want: nil,
			after: map[string]string{
				"/test/key1": "val1",
			},
		},
		{
			name: "delete key with empty key",
			preset: map[string]string{
				"/test/": "val",
			},
			args: args{
				key:    []byte(""),
				prevKV: true,
			},
			want: nil,
			after: map[string]string{
				"/test/": "val",
			},
		},
		{
			name: "delete key with nil key",
			preset: map[string]string{
				"/test/": "val",
			},
			args: args{
				key:    nil,
				prevKV: true,
			},
			want: nil,
			after: map[string]string{
				"/test/": "val",
			},
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)
			_, client, closeFunc := testutil.StartEtcd(t, nil)
			defer closeFunc()

			etcd := NewEtcd(EtcdParam{
				KV:       client,
				RootPath: "/test",
			}, zap.NewNop())

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			got, err := etcd.Delete(context.Background(), tt.args.key, tt.args.prevKV)

			// check
			if tt.wantErr {
				re.Error(err)
			} else {
				re.NoError(err)
				re.Equal(tt.want, got)
			}
			resp, err := kv.Get(context.Background(), "/test", clientv3.WithPrefix())
			re.NoError(err)
			re.Equal(len(tt.after), len(resp.Kvs))
			for _, kvs := range resp.Kvs {
				re.Equal(tt.after[string(kvs.Key)], string(kvs.Value))
			}
		})
	}
}

func TestEtcd_BatchDelete(t *testing.T) {
	type fields struct {
		newCmpFunc func() clientv3.Cmp
		maxTxnOps  uint
	}
	type args struct {
		keys   [][]byte
		prevKV bool
		inTxn  bool
	}
	tests := []struct {
		name    string
		preset  map[string]string
		fields  fields
		args    args
		want    []KeyValue
		after   map[string]string
		wantErr bool
		errMsg  string
	}{
		{
			name: "delete existing key",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			args: args{
				keys: [][]byte{[]byte("key1"), []byte("key2")},
			},
			want:  nil,
			after: map[string]string{},
		},
		{
			name: "delete existing key and get prevKV",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			args: args{
				keys:   [][]byte{[]byte("key1"), []byte("key2")},
				prevKV: true,
			},
			want: []KeyValue{
				{
					Key:   []byte("key1"),
					Value: []byte("val1"),
				},
				{
					Key:   []byte("key2"),
					Value: []byte("val2"),
				},
			},
			after: map[string]string{},
		},
		{
			name: "delete nonexistent key",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			args: args{
				keys:   [][]byte{[]byte("key0")},
				prevKV: true,
			},
			want: []KeyValue{},
			after: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
		},
		{
			name: "delete nonexistent key and existing key and empty key",
			preset: map[string]string{
				"/test/":     "val",
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			args: args{
				keys:   [][]byte{[]byte("key0"), []byte("key1"), []byte(""), nil},
				prevKV: true,
			},
			want: []KeyValue{
				{
					Key:   []byte("key1"),
					Value: []byte("val1"),
				},
			},
			after: map[string]string{
				"/test/":     "val",
				"/test/key2": "val2",
			},
		},
		{
			name: "delete with empty list",
			preset: map[string]string{
				"/test/key0": "val0",
			},
			args: args{
				keys: [][]byte{},
			},
			want: nil,
			after: map[string]string{
				"/test/key0": "val0",
			},
		},
		{
			name: "delete when transaction failed",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			fields: fields{
				newCmpFunc: alwaysFailedTxnFunc,
			},
			args: args{
				keys:   [][]byte{[]byte("key0"), []byte("key1")},
				prevKV: true,
			},
			after: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			wantErr: true,
			errMsg:  "etcd transaction failed",
		},
		{
			name: "delete in multiple transactions",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			fields: fields{
				maxTxnOps: 1,
			},
			args: args{
				keys:   [][]byte{[]byte("key1"), []byte("key2")},
				prevKV: true,
			},
			want: []KeyValue{
				{
					Key:   []byte("key1"),
					Value: []byte("val1"),
				},
				{
					Key:   []byte("key2"),
					Value: []byte("val2"),
				},
			},
			after: map[string]string{},
		},
		{
			name: "delete when exceeding maxTxnOps",
			preset: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
			fields: fields{
				maxTxnOps: 1,
			},
			args: args{
				keys:   [][]byte{[]byte("key1"), []byte("key2")},
				prevKV: true,
				inTxn:  true,
			},
			wantErr: true,
			errMsg:  "too many txn operations",
			after: map[string]string{
				"/test/key1": "val1",
				"/test/key2": "val2",
			},
		},
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)

			var cfg *embed.Config
			if tt.fields.maxTxnOps != 0 {
				cfg = testutil.NewEtcdConfig(t)
				cfg.MaxTxnOps = tt.fields.maxTxnOps
			}
			_, client, closeFunc := testutil.StartEtcd(t, cfg)
			defer closeFunc()

			etcd := NewEtcd(EtcdParam{
				KV:        client,
				RootPath:  "/test",
				CmpFunc:   tt.fields.newCmpFunc,
				MaxTxnOps: tt.fields.maxTxnOps,
			}, zap.NewNop())

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			got, err := etcd.BatchDelete(context.Background(), tt.args.keys, tt.args.prevKV, tt.args.inTxn)

			// check
			if tt.wantErr {
				re.ErrorContains(err, tt.errMsg)
			} else {
				re.NoError(err)
				re.Equal(tt.want, got)
			}
			resp, err := kv.Get(context.Background(), "/test", clientv3.WithPrefix())
			re.NoError(err)
			re.Equal(len(tt.after), len(resp.Kvs))
			for _, kvs := range resp.Kvs {
				re.Equal(tt.after[string(kvs.Key)], string(kvs.Value))
			}
		})
	}
}

func alwaysFailedTxnFunc() clientv3.Cmp {
	return clientv3.Compare(clientv3.Value("key-should-not-be-set"), "=", "value-should-not-be-set")
}
