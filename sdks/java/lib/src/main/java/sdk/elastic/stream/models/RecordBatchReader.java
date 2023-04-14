package sdk.elastic.stream.models;

import java.nio.ByteBuffer;
import java.util.List;
import sdk.elastic.stream.flatc.header.StatusT;

/**
 * RecordBatchReader is a wrapper of record batches when fetching from data nodes.
 */
public class RecordBatchReader {
    /**
     * Status of the fetch result.
     */
    private StatusT statusT;
    /**
     * A ByteBuffer which stores the bytes of the record batches.
     */
    private ByteBuffer[] payloadBuffers;
    /**
     * The number of record batches that can be extracted from the bytebuffers.
     */
    private int batchCount;

    public RecordBatchReader(StatusT statusT, ByteBuffer[] payloadBuffers, int batchCount) {
        this.statusT = statusT;
        this.payloadBuffers = payloadBuffers;
        this.batchCount = batchCount;
    }

    /**
     * Get the status of the fetch result.
     *
     * @return status
     */
    public StatusT getStatusT() {
        return statusT;
    }

    /**
     * Get the record batches. Note that position of the payload buffer will be changed after this call.
     *
     * @return record batches
     */
    public List<RecordBatch> getRecordBatches() {
        return RecordBatch.decode(payloadBuffers[0], batchCount);
    }
}
