package com.automq.sdk.elastic.stream.api;

import java.time.Duration;

public interface ClientBuilder {

    ClientBuilder endpoint(String endpoint);

    ClientBuilder connectionTimeout(Duration duration);

    ClientBuilder channelMaxIdleTime(Duration duration);

    ClientBuilder heartbeatInterval(Duration duration);

    ClientBuilder clientAsyncSemaphoreValue(int semaphore);

}
