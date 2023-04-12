package sdk.elastic.stream.apis.manager;

import com.google.common.util.concurrent.ListenableFuture;
import java.nio.ByteBuffer;
import java.util.Iterator;
import java.util.Map;
import sdk.elastic.stream.grpc.kv.LoadResponse;
import sdk.elastic.stream.grpc.kv.StoreResponse;

/**
 * The manager for KV service.
 */
public interface KvManager {
    /**
     * Store the key-value pairs.
     *
     * @param kvMap the key-value maps to store.
     * @param prefix the prefix of the key-value pairs.
     * @return the future of the store operation.
     */
    ListenableFuture<StoreResponse> save(Map<String, ByteBuffer> kvMap, ByteBuffer prefix);
    /**
     * Delete the key-value pairs.
     *
     * @param keys the keys to delete.
     * @param prefix the prefix of the key-value pairs.
     * @return the future of the delete operation.
     */
    ListenableFuture<StoreResponse> delete(Iterator<String> keys, ByteBuffer prefix);
    /**
     * Load the key-value pairs.
     *
     * @param keys the keys to load.
     * @param prefix the prefix of the key-value pairs.
     * @return the future of the load operation.
     */
    ListenableFuture<LoadResponse> load(Iterator<String> keys, ByteBuffer prefix);
}
