package sdk.elastic.stream.apis;

import java.time.Duration;

import static com.google.common.base.Preconditions.checkNotNull;

public class ClientConfigurationBuilder {
    private String placementManagerEndpoint;
    private Duration connectionTimeout = Duration.ofSeconds(10);
    private Duration channelMaxIdleTime = Duration.ofMinutes(10);
    private int clientAsyncSemaphoreValue = 65535;
    /**
     * Heartbeat interval to keep the client. Default is 10 seconds.
     */
    private Duration heartbeatInterval = Duration.ofSeconds(10);
    private int streamCacheSize = 1000;

    /**
     * Set the endpoint of the placement manager.
     * @param endpoint address of the placement manager, with the form of "ip:port".
     * @return the client configuration builder instance.
     */
    public ClientConfigurationBuilder setPmEndpoint(String endpoint) {
        checkNotNull(endpoint, "endpoint should not be null");
        this.placementManagerEndpoint = endpoint;
        return this;
    }

    public ClientConfigurationBuilder setHeartBeatInterval(Duration heartbeatInterval) {
        checkNotNull(heartbeatInterval, "heartbeatInterval should not be null");
        this.heartbeatInterval = heartbeatInterval;
        return this;
    }

    /**
     * Set the connection timeout.
     * @param timeout timeout
     * @return the client configuration builder instance.
     */
    public ClientConfigurationBuilder setConnectionTimeout(Duration timeout) {
        checkNotNull(timeout, "connectionTimeout should not be null");
        this.connectionTimeout = timeout;
        return this;
    }

    /**
     * Set the channel max idle time.
     * @param channelMaxIdleTime idle time.
     * @return the client configuration builder instance.
     */
    public ClientConfigurationBuilder setChannelMaxIdleTime(Duration channelMaxIdleTime) {
        checkNotNull(channelMaxIdleTime, "channelMaxIdleTime should not be null");
        this.channelMaxIdleTime = channelMaxIdleTime;
        return this;
    }

    /**
     * Set the client async semaphore value.
     *
     * @param clientAsyncSemaphoreValue semaphore value.
     * @return the client configuration builder instance.
     */
    public ClientConfigurationBuilder setClientAsyncSemaphoreValue(int clientAsyncSemaphoreValue) {
        this.clientAsyncSemaphoreValue = clientAsyncSemaphoreValue;
        return this;
    }

    /**
     * Set the stream cache size.
     *
     * @param streamCacheSize cache size.
     * @return the client configuration builder instance.
     */
    public ClientConfigurationBuilder setStreamCacheSize(int streamCacheSize) {
        this.streamCacheSize = streamCacheSize;
        return this;
    }

    /**
     * Build the client configuration.
     *
     * @return the client configuration.
     */
    public ClientConfiguration build() {
        checkNotNull(placementManagerEndpoint, "endpoints should not be null");
        return new ClientConfiguration(placementManagerEndpoint, connectionTimeout, channelMaxIdleTime, clientAsyncSemaphoreValue, heartbeatInterval, streamCacheSize);
    }
}
