package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.KVClient;
import com.automq.elasticstream.client.api.KeyValue;
import com.automq.elasticstream.client.grpc.kv.EventType;
import com.automq.elasticstream.client.grpc.kv.Item;
import com.automq.elasticstream.client.grpc.kv.KVGrpc;
import com.automq.elasticstream.client.grpc.kv.LoadRequest;
import com.automq.elasticstream.client.grpc.kv.StoreRequest;
import com.automq.elasticstream.client.utils.FutureUtils;
import com.google.protobuf.ByteString;
import io.grpc.Channel;
import io.grpc.Grpc;
import io.grpc.InsecureChannelCredentials;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

public class DefaultKVClient implements KVClient {
    private static final Logger log = LoggerFactory.getLogger(DefaultKVClient.class);

    private final KVGrpc.KVFutureStub kvFutureStub;

    public DefaultKVClient(Channel channel) {
        this.kvFutureStub = KVGrpc.newFutureStub(channel);
    }

    public DefaultKVClient(String endpoint) {
        this(Grpc.newChannelBuilder(endpoint, InsecureChannelCredentials.create())
            .build());
    }

    @Override
    public CompletableFuture<Void> putKV(List<KeyValue> keyValues) {
        StoreRequest.Builder requestBuilder = StoreRequest.newBuilder();
        keyValues.forEach(kv -> {
            Item item = Item.newBuilder()
                .setKind(EventType.PUT)
                .setName(ByteString.copyFrom(kv.key().getBytes(StandardCharsets.ISO_8859_1)))
                .setPayload(ByteString.copyFrom(kv.value().slice()))
                .build();
            requestBuilder.addChanges(item);
        });

        return FutureUtils.buildCompletableFuture(kvFutureStub.store(requestBuilder.build()))
            .thenApply(r -> null);
    }

    @Override
    public CompletableFuture<List<KeyValue>> getKV(List<String> keys) {
        LoadRequest.Builder requestBuilder = LoadRequest.newBuilder();
        for (String key : keys) {
            requestBuilder.addNames(ByteString.copyFrom(key.getBytes(StandardCharsets.ISO_8859_1)));
        }

        return FutureUtils.buildCompletableFuture(kvFutureStub.load(requestBuilder.build()))
            .thenApply(r -> {
                List<KeyValue> keyValues = new java.util.ArrayList<>(keys.size());
                for (int i = 0; i < r.getItemsList().size(); i++) {
                    if (r.getItems(i).hasError()) {
                        log.warn("fail to load value for key {}, error msg: {}", keys.get(i), r.getItems(i).getError());
                        continue;
                    }
                    keyValues.add(KeyValue.of(r.getItems(i).getName().toString(StandardCharsets.ISO_8859_1), r.getItems(i).getPayload().asReadOnlyByteBuffer()));
                }
                return keyValues;
            });
    }

    @Override
    public CompletableFuture<Void> delKV(List<String> keys) {
        StoreRequest.Builder requestBuilder = StoreRequest.newBuilder();
        for (String key : keys) {
            Item item = Item.newBuilder()
                .setKind(EventType.DELETE)
                .setName(ByteString.copyFrom(key.getBytes(StandardCharsets.ISO_8859_1)))
                .build();
            requestBuilder.addChanges(item);
        }

        return FutureUtils.buildCompletableFuture(kvFutureStub.store(requestBuilder.build()))
            .thenApply(r -> null);
    }
}
