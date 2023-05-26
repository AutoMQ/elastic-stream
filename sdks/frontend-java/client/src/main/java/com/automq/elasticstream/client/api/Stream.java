package com.automq.elasticstream.client.api;

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
     * @param startOffset  start offset, if the startOffset in middle of a recordBatch, the recordBatch will be returned.
     * @param endOffset exclusive end offset, if the endOffset in middle of a recordBatch, the recordBatch will be returned.
     * @param maxBytesHint max fetch data size hint, the real return data size may be larger than maxBytesHint.
     * @return - complete success with {@link FetchResult}, when fetch success.
     * - complete exception with {@link ElasticStreamClientException.FetchOutOfBoundExceptionElastic}, when startOffset is bigger than stream end offset.
     */
    CompletableFuture<FetchResult> fetch(long startOffset, long endOffset, int maxBytesHint);

    /**
     * Trim stream.
     *
     * @param newStartOffset new start offset.
     * @return - complete success with async {@link Void}, when trim success.
     * - complete exception with {@link ElasticStreamClientException}, when trim fail.
     */
    CompletableFuture<Void> trim(long newStartOffset);

    /**
     * Close the stream.
     */
    CompletableFuture<Void> close();

    /**
     * Destroy stream.
     */
    CompletableFuture<Void> destroy();
}
