package com.automq.sdk.elastic.stream.api;

import java.util.List;

public interface FetchResult {

    /**
     * Get fetched RecordBatch list.
     *
     * @return {@link RecordBatchWithContext} list.
     */
    List<RecordBatchWithContext> recordBatchList();

}
