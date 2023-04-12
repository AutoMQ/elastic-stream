package sdk.elastic.stream.integration.client.kv;

import com.google.common.util.concurrent.FutureCallback;
import com.google.common.util.concurrent.Futures;
import com.google.common.util.concurrent.ListenableFuture;
import com.google.common.util.concurrent.MoreExecutors;
import io.grpc.Grpc;
import io.grpc.InsecureChannelCredentials;
import io.grpc.ManagedChannel;
import java.nio.ByteBuffer;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import sdk.elastic.stream.apis.manager.KvManager;
import sdk.elastic.stream.client.kv.KvManagerImpl;
import sdk.elastic.stream.client.route.Address;
import sdk.elastic.stream.grpc.kv.ErrorType;
import sdk.elastic.stream.grpc.kv.LoadResponse;
import sdk.elastic.stream.grpc.kv.StoreResponse;

class KvManagerImplTest {
    private static KvManager kvManager;
    private static ManagedChannel channel;
    private static final Address defaultPmAddress = Address.fromAddress(System.getProperty("pm.address"));

    @BeforeAll
    public static void setUp() {
        channel = Grpc.newChannelBuilder(defaultPmAddress.getAddress(), InsecureChannelCredentials.create())
            .build();
        kvManager = new KvManagerImpl(channel);
    }

    @Test
    void storeAndLoad() {
        Map<String, ByteBuffer> kvMap = new HashMap<>();
        kvMap.put("key1", ByteBuffer.wrap("value1".getBytes()));
        kvMap.put("key2", ByteBuffer.wrap("value2".getBytes()));
        ByteBuffer prefix = null;

        ListenableFuture<LoadResponse> loadFuture = Futures.transformAsync(kvManager.save(kvMap, prefix),
            (StoreResponse response) -> kvManager.load(kvMap.keySet().iterator(), prefix), MoreExecutors.directExecutor());
        Futures.addCallback(loadFuture, new FutureCallback<LoadResponse>() {
            @Override
            public void onSuccess(LoadResponse response) {
                Assertions.assertEquals(response.getItemsCount(), 2);
                Assertions.assertEquals(response.getItems(0).getPayload().toString(), "value1");
                Assertions.assertEquals(response.getItems(0).getPayload().toString(), "value2");
            }

            @Override
            public void onFailure(Throwable t) {
                Assertions.fail(t);
            }
        }, MoreExecutors.directExecutor());
    }

    @Test
    void loadNotFound() {
        Map<String, ByteBuffer> kvMap = new HashMap<>();
        kvMap.put("key1", ByteBuffer.wrap("value1".getBytes()));
        kvMap.put("key2", ByteBuffer.wrap("value2".getBytes()));
        ByteBuffer prefix = null;

        ListenableFuture<LoadResponse> loadFuture = kvManager.load(kvMap.keySet().iterator(), prefix);
        Futures.addCallback(loadFuture, new FutureCallback<LoadResponse>() {
            @Override
            public void onSuccess(LoadResponse response) {
                Assertions.assertEquals(response.getItemsCount(), 2);
                Assertions.assertEquals(response.getItems(0).getError().getType(), ErrorType.NOT_FOUND);
                Assertions.assertTrue(response.getItems(0).getPayload().isEmpty());
                Assertions.assertEquals(response.getItems(0).getPayload().toString(), "value2");
            }

            @Override
            public void onFailure(Throwable t) {
                Assertions.fail(t);
            }
        }, MoreExecutors.directExecutor());
    }

    @Test
    void storeAndDelete() {
        Map<String, ByteBuffer> kvMap = new HashMap<>();
        kvMap.put("key1", ByteBuffer.wrap("value1".getBytes()));
        kvMap.put("key2", ByteBuffer.wrap("value2".getBytes()));
        ByteBuffer prefix = null;

        ListenableFuture<StoreResponse> loadFuture = Futures.transformAsync(kvManager.save(kvMap, prefix),
            (StoreResponse response) -> kvManager.delete(Collections.singletonList("key1").iterator(), prefix), MoreExecutors.directExecutor());
        ListenableFuture<LoadResponse> future = Futures.transformAsync(loadFuture, (StoreResponse response) -> kvManager.load(kvMap.keySet().iterator(), prefix),
            MoreExecutors.directExecutor());
        Futures.addCallback(future, new FutureCallback<LoadResponse>() {
            @Override
            public void onSuccess(LoadResponse response) {
                Assertions.assertEquals(response.getItemsCount(), 2);
                Assertions.assertEquals(response.getItems(0).getError().getType(), ErrorType.NOT_FOUND);
                Assertions.assertTrue(response.getItems(0).getPayload().isEmpty());
                Assertions.assertEquals(response.getItems(0).getPayload().toString(), "value2");
            }

            @Override
            public void onFailure(Throwable t) {
                Assertions.fail(t);
            }
        }, MoreExecutors.directExecutor());
    }

    @AfterAll
    public static void tearDown() {
        channel.shutdownNow();
    }
}