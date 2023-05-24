package com.automq.elasticstream.client.api;

import com.automq.elasticstream.client.DefaultClientBuilder;

/**
 * Elastic Stream client.
 */
public interface Client {

    static ClientBuilder builder() {
       return new DefaultClientBuilder();
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
