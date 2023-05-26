package com.automq.elasticstream.client.api;

public interface RecordBatchWithContext extends RecordBatch {

    /**
     * Get record batch base offset.
     *
     * @return base offset.
     */
    long baseOffset();

    /**
     * Get record batch exclusive last offset.
     *
     * @return exclusive last offset.
     */
    long lastOffset();
}
