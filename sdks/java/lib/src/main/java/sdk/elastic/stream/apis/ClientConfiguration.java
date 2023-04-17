package sdk.elastic.stream.apis;

import java.time.Duration;

public class ClientConfiguration {
    private final String placementManagerEndpoint;
    private final Duration connectionTimeout;
    /**
     * IdleStateEvent will be triggered when neither read nor write was performed for
     * the specified period of this time. Specify {@code 0} to disable
     */
    private final Duration channelMaxIdleTime;
    /**
     * Heartbeat interval to keep the client. Default is 10 seconds.
     */
    private final Duration heartbeatInterval;
    private final int clientAsyncSemaphoreValue;
    /**
     * The size of the stream cache. Default is 1000.
     */
    private final int streamCacheSize;

    ClientConfiguration(String endpoint, Duration connectionTimeout, Duration channelMaxIdleTime,
        int clientAsyncSemaphoreValue, Duration heartbeatInterval, int streamCacheSize) {
        this.placementManagerEndpoint = endpoint;
        this.connectionTimeout = connectionTimeout;
        this.channelMaxIdleTime = channelMaxIdleTime;
        this.clientAsyncSemaphoreValue = clientAsyncSemaphoreValue;
        this.heartbeatInterval = heartbeatInterval;
        this.streamCacheSize = streamCacheSize;
    }

    public static ClientConfigurationBuilder newBuilder() {
        return new ClientConfigurationBuilder();
    }

    public String getPlacementManagerEndpoint() {
        return placementManagerEndpoint;
    }

    public Duration getHeartbeatInterval() {
        return heartbeatInterval;
    }

    public Duration getConnectionTimeout() {
        return connectionTimeout;
    }

    public Duration getChannelMaxIdleTime() {
        return channelMaxIdleTime;
    }

    public int getClientAsyncSemaphoreValue() {
        return clientAsyncSemaphoreValue;
    }

    public int getStreamCacheSize() {
        return streamCacheSize;
    }
}
