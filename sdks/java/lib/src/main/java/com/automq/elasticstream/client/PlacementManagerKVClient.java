package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.KVClient;
import com.automq.elasticstream.client.api.KeyValue;

import java.util.List;
import java.util.concurrent.CompletableFuture;

public class PlacementManagerKVClient implements KVClient {
    private final String endpoint;

    public PlacementManagerKVClient(String endpoint) {
        this.endpoint = endpoint;
    }

    @Override
    public CompletableFuture<Void> putKV(List<KeyValue> keyValues) {
        throw new UnsupportedOperationException();
    }

    @Override
    public CompletableFuture<List<KeyValue>> getKV(List<String> keys) {
        throw new UnsupportedOperationException();
    }

    @Override
    public CompletableFuture<Void> delKV(List<String> keys) {
        throw new UnsupportedOperationException();
    }
}
