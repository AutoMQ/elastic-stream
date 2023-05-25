package com.automq.elasticstream.client.api;

import java.nio.ByteBuffer;
import java.util.Map;

/**
 * Record batch.
 */
public interface RecordBatch {

    /**
     * Get payload record count.
     *
     * @return record count.
     */
    int count();

    /**
     * Get min timestamp of records.
     *
     * @return min timestamp of records.
     */
    long baseTimestamp();

    /**
     * Get record batch extension properties.
     *
     * @return batch extension properties.
     */
    Map<String, String> properties();

    /**
     * Get raw payload.
     *
     * @return raw payload.
     */
    ByteBuffer rawPayload();
}
