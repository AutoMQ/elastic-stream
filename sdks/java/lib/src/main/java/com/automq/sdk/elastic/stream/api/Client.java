package com.automq.sdk.elastic.stream.api;

/**
 * Elastic Stream client.
 */
public interface Client {

    static ClientBuilder builder() {
        throw new UnsupportedOperationException();
    }

    /**
     * Get stream client.
     *
     * @return {@link StreamClient}
     */
    StreamClient streamClient();

    /**
     * Get KV client.
     *
     * @return {@link KVClient}
     */
    KVClient kvClient();
}
