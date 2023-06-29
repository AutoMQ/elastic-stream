package fbutil

import (
	"sync"

	"github.com/bytedance/gopkg/lang/mcache"
	flatbuffers "github.com/google/flatbuffers/go"
)

var builderPool = sync.Pool{
	New: func() interface{} { return flatbuffers.NewBuilder(1024) },
}

// Packable is an interface for flatbuffer packable object
type Packable interface {
	Pack(builder *flatbuffers.Builder) flatbuffers.UOffsetT
}

// Marshal encodes a Packable FlatBuffer object into a byte slice
// The returned byte slice should be freed after use.
func Marshal(packable Packable) []byte {
	builder := builderPool.Get().(*flatbuffers.Builder)
	defer func() {
		builder.Reset()
		builderPool.Put(builder)
	}()

	builder.Finish(packable.Pack(builder))
	bytes := builder.FinishedBytes()

	result := mcache.Malloc(len(bytes))
	copy(result, bytes)
	return result
}
