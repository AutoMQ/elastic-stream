package com.automq.elasticstream.client.api;

import java.util.List;

public interface FetchResult {

    /**
     * Get fetched RecordBatch list.
     *
     * @return {@link RecordBatchWithContext} list.
     */
    List<RecordBatchWithContext> recordBatchList();

    /**
     * Free fetch result backend memory.
     */
    default void free() {};
}
