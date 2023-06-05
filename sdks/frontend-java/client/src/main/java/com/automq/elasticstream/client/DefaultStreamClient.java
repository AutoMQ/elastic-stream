package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.CreateStreamOptions;
import com.automq.elasticstream.client.api.OpenStreamOptions;
import com.automq.elasticstream.client.api.Stream;
import com.automq.elasticstream.client.api.StreamClient;
import com.automq.elasticstream.client.jni.Frontend;

import java.util.concurrent.CompletableFuture;

public class DefaultStreamClient implements StreamClient {
    private final String endpoint;
    private final Frontend frontend;

    public DefaultStreamClient(String endpoint) {
        this.endpoint = endpoint;
        this.frontend = new Frontend(endpoint);
    }

    @Override
    public CompletableFuture<Stream> createAndOpenStream(CreateStreamOptions options) {
        return frontend.create(options.replicaCount(), options.replicaCount() / 2 + 1, Long.MAX_VALUE)
                .thenCompose(streamId -> openStream(streamId, OpenStreamOptions.newBuilder().epoch(options.epoch()).build()));
    }

    @Override
    public CompletableFuture<Stream> openStream(long streamId, OpenStreamOptions options) {
        return frontend.open(streamId, options.epoch()).thenApply(s -> new DefaultStream(streamId, s));
    }

    public String toString() {
        return "StreamClient(endpoint=" + this.endpoint + ")";
    }
}
