package id

import (
	"context"
	"sort"
	"sync"
	"testing"

	"github.com/stretchr/testify/require"
	clientv3 "go.etcd.io/etcd/client/v3"
	"go.uber.org/zap"

	"github.com/AutoMQ/placement-manager/pkg/util/testutil"
)

func TestEtcdAllocator_Alloc(t *testing.T) {
	t.Parallel()

	_, client, closeFunc := testutil.StartEtcd(t, nil)
	defer closeFunc()

	// run twice to test the allocator can recover from the previous allocation
	_, end1 := testEtcdAlloc(t, client)
	start2, _ := testEtcdAlloc(t, client)
	require.Less(t, end1, start2)
}

func testEtcdAlloc(t *testing.T, kv clientv3.KV) (start, end uint64) {
	re := require.New(t)

	allocator := NewEtcdAllocator(&EtcdAllocatorParam{
		KV:       kv,
		CmpFunc:  func() clientv3.Cmp { return clientv3.Compare(clientv3.CreateRevision("not-exist-key"), "=", 0) },
		RootPath: "test-root",
		Key:      "test-key",
		Step:     2,
	}, zap.NewNop())

	// start multi go-routine to alloc
	ids := make([]uint64, 100)
	wg := sync.WaitGroup{}
	wg.Add(len(ids))
	for i := range ids {
		go func(i int) {
			alloc, err := allocator.Alloc(context.Background())
			ids[i] = alloc
			re.NoError(err)
			wg.Done()
		}(i)
	}
	wg.Wait()

	sort.Slice(ids, func(i, j int) bool {
		return ids[i] < ids[j]
	})
	for i := 1; i < len(ids); i++ {
		re.Equal(ids[i-1]+1, ids[i])
	}
	return ids[0], ids[len(ids)-1]
}

func TestEtcdAllocator_AllocN(t *testing.T) {
	t.Parallel()

	_, client, closeFunc := testutil.StartEtcd(t, nil)
	defer closeFunc()

	// run twice to test the allocator can recover from the previous allocation
	_, end1 := testEtcdAllocN(t, client)
	start2, _ := testEtcdAllocN(t, client)
	require.Less(t, end1, start2)
}

func testEtcdAllocN(t *testing.T, kv clientv3.KV) (start, end uint64) {
	re := require.New(t)

	allocator := NewEtcdAllocator(&EtcdAllocatorParam{
		KV:       kv,
		CmpFunc:  func() clientv3.Cmp { return clientv3.Compare(clientv3.CreateRevision("not-exist-key"), "=", 0) },
		RootPath: "test-root",
		Key:      "test-key",
		Step:     42,
	}, zap.NewNop())

	// start multi go-routine to allocN
	ids := make([][]uint64, 100)
	wg := sync.WaitGroup{}
	wg.Add(len(ids))
	for i := range ids {
		go func(i int) {
			alloc, err := allocator.AllocN(context.Background(), 10)
			ids[i] = alloc
			re.NoError(err)
			wg.Done()
		}(i)
	}
	wg.Wait()

	all := make([]uint64, 0, len(ids)*10)
	for _, alloc := range ids {
		re.Equal(10, len(alloc))
		for i := 1; i < len(alloc); i++ {
			re.Equal(alloc[i-1]+1, alloc[i])
		}
		all = append(all, alloc...)
	}
	sort.Slice(all, func(i, j int) bool {
		return all[i] < all[j]
	})
	for i := 1; i < len(all); i++ {
		re.Equal(all[i-1]+1, all[i])
	}
	return all[0], all[len(all)-1]
}

func TestEtcdAllocator_Reset(t *testing.T) {
	t.Parallel()
	re := require.New(t)

	_, client, closeFunc := testutil.StartEtcd(t, nil)
	defer closeFunc()

	allocator := NewEtcdAllocator(&EtcdAllocatorParam{
		KV:       client,
		CmpFunc:  func() clientv3.Cmp { return clientv3.Compare(clientv3.CreateRevision("not-exist-key"), "=", 0) },
		RootPath: "test-root",
		Key:      "test-key",
		Start:    1234,
		Step:     10,
	}, zap.NewNop())

	ids, err := allocator.AllocN(context.Background(), 42)
	re.NoError(err)
	re.Equal(42, len(ids))
	re.Equal(uint64(1234), ids[0])
	re.Equal(uint64(1275), ids[41])

	err = allocator.Reset(context.Background())
	re.NoError(err)

	ids, err = allocator.AllocN(context.Background(), 84)
	re.NoError(err)
	re.Equal(84, len(ids))
	re.Equal(uint64(1234), ids[0])
	re.Equal(uint64(1317), ids[83])
}
