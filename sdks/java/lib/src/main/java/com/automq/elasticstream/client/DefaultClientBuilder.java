package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.Client;
import com.automq.elasticstream.client.api.ClientBuilder;

import java.time.Duration;

public class DefaultClientBuilder implements ClientBuilder {
    private String endpoint;

    @Override
    public ClientBuilder endpoint(String endpoint) {
        this.endpoint = endpoint;
        return this;
    }

    @Override
    public ClientBuilder connectionTimeout(Duration duration) {
        return this;
    }

    @Override
    public ClientBuilder channelMaxIdleTime(Duration duration) {
        return this;
    }

    @Override
    public ClientBuilder heartbeatInterval(Duration duration) {
        return this;
    }

    @Override
    public ClientBuilder clientAsyncSemaphoreValue(int semaphore) {
        return this;
    }

    @Override
    public Client build() {
        return new DefaultClient(endpoint);
    }
}
