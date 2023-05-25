package com.automq.elasticstream.client.api;

import java.util.List;
import java.util.concurrent.CompletableFuture;

/**
 * Light KV client, support light & simple kv operations.
 */
public interface KVClient {
    /**
     * Put key value.
     *
     * @param keyValues {@link KeyValue} list.
     * @return async put result.
     */
    CompletableFuture<Void> putKV(List<KeyValue> keyValues);

    /**
     * Get value by key.
     *
     * @param keys key list.
     * @return {@link KeyValue} list.
     */
    CompletableFuture<List<KeyValue>> getKV(List<String> keys);

    /**
     * Delete key value by key.
     *
     * @param keys key list.
     * @return async delete result.
     */
    CompletableFuture<Void> delKV(List<String> keys);
}
