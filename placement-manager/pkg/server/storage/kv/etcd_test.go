package kv

import (
	"context"
	"testing"

	"github.com/stretchr/testify/require"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.uber.org/goleak"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/pkg/util/testutil"
)

func TestMain(m *testing.M) {
	goleak.VerifyTestMain(m)
}

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
			_, client, closeFunc := testutil.StartEtcd(t)
			defer closeFunc()

			etcd := NewEtcd(client, "/test", zap.NewNop(), nil)

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
	}
	type args struct {
		keys [][]byte
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
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)
			_, client, closeFunc := testutil.StartEtcd(t)
			defer closeFunc()

			etcd := NewEtcd(client, "/test", zap.NewNop(), tt.fields.newCmpFunc)

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			got, err := etcd.BatchGet(context.Background(), tt.args.keys)

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
			_, client, closeFunc := testutil.StartEtcd(t)
			defer closeFunc()

			etcd := NewEtcd(client, "/test", zap.NewNop(), nil)

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
	}
	type args struct {
		kvs    []KeyValue
		prevKV bool
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
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)
			_, client, closeFunc := testutil.StartEtcd(t)
			defer closeFunc()

			etcd := NewEtcd(client, "/test", zap.NewNop(), tt.fields.newCmpFunc)

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			got, err := etcd.BatchPut(context.Background(), tt.args.kvs, tt.args.prevKV)

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
			_, client, closeFunc := testutil.StartEtcd(t)
			defer closeFunc()

			etcd := NewEtcd(client, "/test", zap.NewNop(), nil)

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
	}
	type args struct {
		keys   [][]byte
		prevKV bool
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
	}
	for _, tt := range tests {
		tt := tt
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			re := require.New(t)
			_, client, closeFunc := testutil.StartEtcd(t)
			defer closeFunc()

			etcd := NewEtcd(client, "/test", zap.NewNop(), tt.fields.newCmpFunc)

			// prepare
			kv := client.KV
			for k, v := range tt.preset {
				_, err := kv.Put(context.Background(), k, v)
				re.NoError(err)
			}

			// run
			got, err := etcd.BatchDelete(context.Background(), tt.args.keys, tt.args.prevKV)

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
