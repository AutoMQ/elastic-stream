package sdk.elastic.stream.apis;

import java.io.Closeable;
import java.time.Duration;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import sdk.elastic.stream.client.common.ClientId;
import sdk.elastic.stream.client.route.Address;
import sdk.elastic.stream.flatc.header.AppendResultT;
import sdk.elastic.stream.flatc.header.CreateStreamResultT;
import sdk.elastic.stream.flatc.header.StreamT;
import sdk.elastic.stream.models.RecordBatch;

public interface OperationClient extends Closeable {
    /**
     * Create a batch of streams.
     *
     * @param streams A batch of streams to be created.
     * @param timeout request timeout.
     * @return create stream results.
     */
    CompletableFuture<List<CreateStreamResultT>> createStreams(List<StreamT> streams, Duration timeout);

    /**
     * Append a batch to data nodes. Null will be returned if the request failed.
     *
     * @param recordBatch record batch to be appended.
     * @param timeout     request timeout.
     * @return AppendResult for this request. Null if the append request failed.
     */
    CompletableFuture<AppendResultT> appendBatch(RecordBatch recordBatch, Duration timeout);

    /**
     * Fetch batches from data nodes. Null will be returned if the request failed.
     * Note that 'maxBytes' is not a hard limit, it's just a hint to the server.
     *
     * @param streamId    stream id.
     * @param startOffset start offset.
     * @param minBytes    minimum bytes to be fetched.
     * @param maxBytes    maximum bytes to be fetched.
     * @param timeout     request timeout.
     * @return RecordBatches for this request.
     */
    CompletableFuture<List<RecordBatch>> fetchBatches(long streamId, long startOffset, int minBytes, int maxBytes,
        Duration timeout);

    /**
     * Get the last writable offset of a stream.
     *
     * @param streamId stream id.
     * @param timeout  request timeout.
     * @return last writable offset of the stream.
     */
    CompletableFuture<Long> getLastWritableOffset(long streamId, Duration timeout);

    /**
     * Send the heartbeat to the server to keep the connection. False will be returned if the request failed.
     *
     * @param address address of the PM or data nodes.
     * @param timeout request timeout.
     * @return true if get a valid heartbeat response.
     */
    CompletableFuture<Boolean> heartbeat(Address address, Duration timeout);

    /**
     * Start the client.
     *
     * @throws Exception
     */
    void start() throws Exception;

    /**
     * Get the client id.
     *
     * @return client id.
     */
    ClientId getClientId();
}
