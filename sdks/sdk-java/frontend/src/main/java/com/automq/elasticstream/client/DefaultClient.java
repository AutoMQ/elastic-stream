package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.Client;
import com.automq.elasticstream.client.api.KVClient;
import com.automq.elasticstream.client.api.StreamClient;

public class DefaultClient implements Client {
    private final KVClient kvClient;
    private final StreamClient streamClient;

    public DefaultClient(String endpoint) {
        this.kvClient = new DefaultKVClient(endpoint);
        this.streamClient = new DefaultStreamClient(endpoint);
    }

    @Override
    public StreamClient streamClient() {
        return streamClient;
    }

    @Override
    public KVClient kvClient() {
        return kvClient;
    }
}
