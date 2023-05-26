package com.automq.elasticstream.client.api;

/**
 * Append RecordBatch to stream result.
 */
public interface AppendResult {

    /**
     * Get record batch base offset.
     *
     * @return record batch base offset.
     */
    long baseOffset();

}
