package sdk.elastic.stream.client.kv;

import com.google.common.util.concurrent.FutureCallback;
import com.google.common.util.concurrent.Futures;
import com.google.common.util.concurrent.ListenableFuture;
import com.google.common.util.concurrent.MoreExecutors;
import com.google.protobuf.ByteString;
import io.grpc.inprocess.InProcessChannelBuilder;
import io.grpc.inprocess.InProcessServerBuilder;
import io.grpc.stub.StreamObserver;
import io.grpc.testing.GrpcCleanupRule;
import io.grpc.util.MutableHandlerRegistry;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Collections;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import org.junit.Rule;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.runner.RunWith;
import org.junit.runners.JUnit4;
import sdk.elastic.stream.apis.manager.KvManager;
import sdk.elastic.stream.grpc.kv.Error;
import sdk.elastic.stream.grpc.kv.ErrorType;
import sdk.elastic.stream.grpc.kv.EventType;
import sdk.elastic.stream.grpc.kv.Item;
import sdk.elastic.stream.grpc.kv.KVGrpc;
import sdk.elastic.stream.grpc.kv.LoadRequest;
import sdk.elastic.stream.grpc.kv.LoadResponse;
import sdk.elastic.stream.grpc.kv.StoreRequest;
import sdk.elastic.stream.grpc.kv.StoreResponse;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotNull;
import static org.junit.jupiter.api.Assertions.fail;

@RunWith(JUnit4.class)
class KvManagerImplTest {
    /**
     * This rule manages automatic graceful shutdown for the registered server at the end of test.
     */
    @Rule
    public final GrpcCleanupRule grpcCleanup = new GrpcCleanupRule();
    private final MutableHandlerRegistry serviceRegistry = new MutableHandlerRegistry();
    private KvManager kvManager;

    @BeforeEach
    void setUp() throws Exception {
        // Generate a unique in-process server name.
        String serverName = InProcessServerBuilder.generateName();
        // Use a mutable service registry for later registering the service impl for each test case.
        grpcCleanup.register(InProcessServerBuilder.forName(serverName)
            .fallbackHandlerRegistry(serviceRegistry).directExecutor().build().start());
        kvManager = new KvManagerImpl(grpcCleanup.register(
            InProcessChannelBuilder.forName(serverName).directExecutor().build()));
        serviceRegistry.addService(new KVImplMock());
    }

    @Test
    void save() {
        Map<String, ByteBuffer> kvMap = new HashMap<String, ByteBuffer>() {{
            put("testKey", ByteBuffer.wrap("testValue".getBytes()));
        }};
        ByteBuffer prefix = null;

        ListenableFuture<StoreResponse> responseListenableFuture = kvManager.save(kvMap, prefix);
        Futures.addCallback(responseListenableFuture, new FutureCallback<StoreResponse>() {
            @Override
            public void onSuccess(StoreResponse result) {
                assertNotNull(result);
            }

            @Override
            public void onFailure(Throwable t) {
                fail(t);
            }
        }, MoreExecutors.directExecutor());
    }

    @Test
    void delete() {
        String toDeleteKey = "key2";
        Map<String, String> kvMap = new HashMap<String, String>() {{
            put("key1", "value1");
            put(toDeleteKey, "value2");
        }};
        ByteBuffer prefix = null;

        Map<String, ByteBuffer> kvRequestMap = new HashMap<>();
        kvMap.forEach((k, v) -> kvRequestMap.put(k, ByteBuffer.wrap(v.getBytes(StandardCharsets.ISO_8859_1))));

        // save and delete one
        ListenableFuture<StoreResponse> loadFuture = Futures.transformAsync(kvManager.save(kvRequestMap, prefix), storeResponse ->
            kvManager.delete(Collections.singletonList(toDeleteKey).iterator(), prefix), MoreExecutors.directExecutor());
        ListenableFuture<LoadResponse> future = Futures.transformAsync(loadFuture, (StoreResponse response) -> kvManager.load(kvMap.keySet().iterator(), prefix),
            MoreExecutors.directExecutor());
        Futures.addCallback(future, new FutureCallback<LoadResponse>() {
            @Override
            public void onSuccess(LoadResponse response) {
                assertEquals(response.getItemsCount(), 2);
                String key1 = response.getItems(0).getName().toString(StandardCharsets.ISO_8859_1);
                assertEquals(kvMap.get(key1), response.getItems(0).getPayload().toString(StandardCharsets.ISO_8859_1));
                assertEquals(ErrorType.NOT_FOUND, response.getItems(1).getError().getType());
            }

            @Override
            public void onFailure(Throwable t) {
                Assertions.fail(t);
            }
        }, MoreExecutors.directExecutor());
    }

    @Test
    void load() {
        Map<String, String> kvMap = new HashMap<String, String>() {{
            put("key1", "value1");
            put("key2", "value2");
        }};
        ByteBuffer prefix = null;

        Map<String, ByteBuffer> kvRequestMap = new HashMap<>();
        kvMap.forEach((k, v) -> kvRequestMap.put(k, ByteBuffer.wrap(v.getBytes(StandardCharsets.ISO_8859_1))));

        // save and load
        ListenableFuture<LoadResponse> future = Futures.transformAsync(kvManager.save(kvRequestMap, prefix), storeResponse ->
            kvManager.load(kvMap.keySet().iterator(), prefix), MoreExecutors.directExecutor());

        Futures.addCallback(future, new FutureCallback<LoadResponse>() {
            @Override
            public void onSuccess(LoadResponse result) {
                assertEquals(kvMap.size(), result.getItemsCount());
                result.getItemsList().forEach(item -> assertEquals(kvMap.get(item.getName().toString(StandardCharsets.ISO_8859_1)),
                    item.getPayload().toString(StandardCharsets.ISO_8859_1)));
            }

            @Override
            public void onFailure(Throwable t) {
                fail(t);
            }
        }, MoreExecutors.directExecutor());
    }

    @Test
    void loadError() {
        Map<String, String> kvMap = new HashMap<String, String>() {{
            put("key1", "value1");
        }};
        ByteBuffer prefix = null;

        Futures.addCallback(kvManager.load(kvMap.keySet().iterator(), prefix), new FutureCallback<LoadResponse>() {
            @Override
            public void onSuccess(LoadResponse result) {
                assertEquals(kvMap.size(), result.getItemsCount());
                result.getItemsList().forEach(item -> assertEquals(ErrorType.NOT_FOUND, item.getError().getType()));
            }

            @Override
            public void onFailure(Throwable t) {
                fail(t);
            }
        }, MoreExecutors.directExecutor());
    }

    private static class KVImplMock extends KVGrpc.KVImplBase {
        private final Map<ByteString, ByteString> storedKvMap = new ConcurrentHashMap<>();

        @Override
        public void store(StoreRequest request, StreamObserver<StoreResponse> responseObserver) {
            for (Item item : request.getChangesList()) {
                if (item.getKind().equals(EventType.PUT)) {
                    storedKvMap.put(item.getName(), item.getPayload());
                } else if (item.getKind().equals(EventType.DELETE)) {
                    storedKvMap.remove(item.getName());
                }
            }
            responseObserver.onNext(StoreResponse.newBuilder().build());
            responseObserver.onCompleted();
        }

        @Override
        public void load(LoadRequest request, StreamObserver<LoadResponse> responseObserver) {
            LoadResponse.Builder builder = LoadResponse.newBuilder();
            for (ByteString key : request.getNamesList()) {
                if (storedKvMap.containsKey(key)) {
                    builder.addItems(Item.newBuilder().setName(key).setPayload(storedKvMap.get(key)).build());
                    continue;
                }
                builder.addItems(Item.newBuilder().setError(Error.newBuilder()
                    .setType(ErrorType.NOT_FOUND)
                    .build()
                ).build());
            }
            responseObserver.onNext(builder.build());
            responseObserver.onCompleted();
        }
    }
}

