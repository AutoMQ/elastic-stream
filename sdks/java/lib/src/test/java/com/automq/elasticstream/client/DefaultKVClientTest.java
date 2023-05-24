package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.KVClient;
import com.automq.elasticstream.client.api.KeyValue;
import com.automq.elasticstream.client.grpc.kv.Error;
import com.automq.elasticstream.client.grpc.kv.ErrorType;
import com.automq.elasticstream.client.grpc.kv.EventType;
import com.automq.elasticstream.client.grpc.kv.Item;
import com.automq.elasticstream.client.grpc.kv.KVGrpc;
import com.automq.elasticstream.client.grpc.kv.LoadRequest;
import com.automq.elasticstream.client.grpc.kv.LoadResponse;
import com.automq.elasticstream.client.grpc.kv.StoreRequest;
import com.automq.elasticstream.client.grpc.kv.StoreResponse;
import com.google.protobuf.ByteString;
import io.grpc.inprocess.InProcessChannelBuilder;
import io.grpc.inprocess.InProcessServerBuilder;
import io.grpc.stub.StreamObserver;
import io.grpc.testing.GrpcCleanupRule;
import io.grpc.util.MutableHandlerRegistry;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutionException;
import java.util.stream.Collectors;
import org.junit.Rule;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.junit.runner.RunWith;
import org.junit.runners.JUnit4;

import static org.junit.Assert.assertEquals;

@RunWith(JUnit4.class)
class DefaultKVClientTest {
    /**
     * This rule manages automatic graceful shutdown for the registered server at the end of test.
     */
    @Rule
    public final GrpcCleanupRule grpcCleanup = new GrpcCleanupRule();
    private final MutableHandlerRegistry serviceRegistry = new MutableHandlerRegistry();
    private KVClient kvClient;

    @BeforeEach
    void setUp() throws IOException {
        // Generate a unique in-process server name.
        String serverName = InProcessServerBuilder.generateName();
        // Use a mutable service registry for later registering the service impl for each test case.
        grpcCleanup.register(InProcessServerBuilder.forName(serverName)
            .fallbackHandlerRegistry(serviceRegistry).directExecutor().build().start());
        kvClient = new DefaultKVClient(grpcCleanup.register(
            InProcessChannelBuilder.forName(serverName).directExecutor().build()));
        serviceRegistry.addService(new KVImplMock());
    }

    @AfterEach
    void tearDown() {
    }

    @Test
    void putKV() throws ExecutionException, InterruptedException {
        List<KeyValue> kvList = new ArrayList<>();
        kvList.add(KeyValue.of("testKey", ByteBuffer.wrap("testValue".getBytes())));

        kvClient.putKV(kvList).get();
    }

    @Test
    void getKV() throws ExecutionException, InterruptedException {
        List<KeyValue> kvList = new ArrayList<>();
        kvList.add(KeyValue.of("key1", ByteBuffer.wrap("value1".getBytes())));
        kvList.add(KeyValue.of("key2", ByteBuffer.wrap("value2".getBytes())));

        kvClient.putKV(kvList)
            .thenCompose(r -> kvClient.getKV(kvList.stream().map(KeyValue::key).collect(Collectors.toList())))
            .thenAccept(loadedList -> {
                assertEquals(kvList, loadedList);
            }).get();
    }

    @Test
    void getKVError() throws ExecutionException, InterruptedException {
        List<KeyValue> kvList = new ArrayList<>();
        kvList.add(KeyValue.of("key1", ByteBuffer.wrap("value1".getBytes())));

        kvClient.getKV(kvList.stream().map(KeyValue::key).collect(Collectors.toList()))
            .thenAccept(loadedList -> {
                assertEquals(0, loadedList.size());
            }).get();
    }

    @Test
    void delKV() throws ExecutionException, InterruptedException {
        String toDeleteKey = "key2";
        List<KeyValue> kvList = new ArrayList<>();
        kvList.add(KeyValue.of("key1", ByteBuffer.wrap("value1".getBytes())));
        kvList.add(KeyValue.of(toDeleteKey, ByteBuffer.wrap("value2".getBytes())));

        kvClient.putKV(kvList)
            .thenCompose(r -> kvClient.delKV(Collections.singletonList(toDeleteKey)))
            .thenCompose(r -> kvClient.getKV(kvList.stream().map(KeyValue::key).collect(Collectors.toList())))
            .thenAccept(loadedList -> {
                assertEquals(1, loadedList.size());
                assertEquals(kvList.get(0), loadedList.get(0));
            }).get();
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