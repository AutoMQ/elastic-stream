package sdk.elastic.stream.client.kv;

import com.google.common.util.concurrent.Futures;
import com.google.protobuf.ByteString;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.mockito.Mock;
import sdk.elastic.stream.apis.manager.KvManager;
import sdk.elastic.stream.grpc.kv.Error;
import sdk.elastic.stream.grpc.kv.ErrorType;
import sdk.elastic.stream.grpc.kv.Item;
import sdk.elastic.stream.grpc.kv.LoadResponse;
import sdk.elastic.stream.grpc.kv.StoreResponse;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.doReturn;
import static org.mockito.Mockito.mock;

class KvCacheTest {
    @Mock
    private KvManager kvManager;
    private static final int CACHE_SIZE = 100;

    @BeforeEach
    void setUp() {
        kvManager = mock(KvManagerImpl.class);
    }

    @Test
    void get() {
        String testKey = "testK1";
        String testValue = "testV1";

        Item item = Item.newBuilder()
            .setError(Error.newBuilder()
                .setType(ErrorType.OK)
                .build())
            .setName(ByteString.copyFrom(testKey, StandardCharsets.ISO_8859_1))
            .setPayload(ByteString.copyFrom(testValue, StandardCharsets.ISO_8859_1))
            .build();
        LoadResponse loadResponse = LoadResponse.newBuilder()
            .addItems(item)
            .build();
        KvCache kvCache = new KvCache(null, kvManager, CACHE_SIZE);

        doReturn(Futures.immediateFuture(loadResponse)).when(kvManager).load(any(), any());
        assertEquals(ByteBuffer.wrap(testValue.getBytes(StandardCharsets.ISO_8859_1)), kvCache.get(testKey));
    }

    @Test
    void getNotFound() {
        String testKey = "testK1";
        String testValue = "testV1";

        Item item = Item.newBuilder()
            .setError(Error.newBuilder()
                .setType(ErrorType.NOT_FOUND)
                .build())
            .setName(ByteString.copyFrom(testKey, StandardCharsets.ISO_8859_1))
            .setPayload(ByteString.copyFrom(testValue, StandardCharsets.ISO_8859_1))
            .build();
        LoadResponse loadResponse = LoadResponse.newBuilder()
            .addItems(item)
            .build();
        KvCache kvCache = new KvCache(null, kvManager, CACHE_SIZE);

        doReturn(Futures.immediateFuture(loadResponse)).when(kvManager).load(any(), any());
        assertNull(kvCache.get(testKey));
    }

    @Test
    void put() {
        String testKey = "testK2";
        String testValue = "testV2";

        StoreResponse storeResponse = StoreResponse.newBuilder()
            .build();
        KvCache kvCache = new KvCache(null, kvManager, CACHE_SIZE);

        doReturn(Futures.immediateFuture(storeResponse)).when(kvManager).save(any(), any());
        kvCache.put(testKey, ByteBuffer.wrap(testValue.getBytes(StandardCharsets.ISO_8859_1)));
        assertEquals(ByteBuffer.wrap(testValue.getBytes(StandardCharsets.ISO_8859_1)), kvCache.get(testKey));
    }

    @Test
    void delete() {
        String testKey = "testK2";
        String testValue = "testV2";

        StoreResponse storeResponse = StoreResponse.newBuilder()
            .build();
        KvCache kvCache = new KvCache(null, kvManager, CACHE_SIZE);

        doReturn(Futures.immediateFuture(storeResponse)).when(kvManager).save(any(), any());
        kvCache.put(testKey, ByteBuffer.wrap(testValue.getBytes(StandardCharsets.ISO_8859_1)));
        assertEquals(1, kvCache.getSize());

        doReturn(Futures.immediateFuture(storeResponse)).when(kvManager).delete(any(), any());
        kvCache.delete(testKey);
        assertEquals(0, kvCache.getSize());
    }
}