package sdk.elastic.stream.api;

import sdk.elastic.stream.api.ElasticStreamClientException.FetchOutOfBoundExceptionElastic;

import java.util.concurrent.CompletableFuture;

/**
 * Record stream.
 */
public interface Stream {

    /**
     * Get stream id
     */
    long streamId();

    /**
     * Get stream start offset.
     */
    long startOffset();

    /**
     * Get stream next append record offset.
     */
    long nextOffset();


    /**
     * Append recordBatch to stream.
     *
     * @param recordBatch {@link RecordBatch}.
     * @return - complete success with async {@link AppendResult}, when append success.
     * - complete exception with {@link ElasticStreamClientException}, when append fail. TODO: specify the exception.
     */
    CompletableFuture<AppendResult> append(RecordBatch recordBatch);

    /**
     * Fetch recordBatch list from stream. Note the startOffset may be in the middle in the first recordBatch.
     *
     * @param startOffset  start offset.
     * @param maxBytesHint max fetch data size hint, the real return data size may be larger than maxBytesHint.
     * @return - complete success with {@link FetchResult}, when fetch success.
     * - complete exception with {@link FetchOutOfBoundExceptionElastic}, when startOffset is bigger than stream end offset.
     */
    CompletableFuture<FetchResult> fetch(long startOffset, int maxBytesHint);

    /**
     * Close the stream.
     */
    CompletableFuture<Void> close();

    /**
     * Destroy stream.
     */
    CompletableFuture<Void> destroy();
}
