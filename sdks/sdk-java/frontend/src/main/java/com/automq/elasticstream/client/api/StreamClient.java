package com.automq.elasticstream.client.api;

import java.util.concurrent.CompletableFuture;

/**
 * Stream client, support stream create and open operation.
 */
public interface StreamClient {
    /**
     * Create and open stream.
     *
     * @param options create stream options.
     * @return {@link Stream}.
     */
    CompletableFuture<Stream> createAndOpenStream(CreateStreamOptions options);

    /**
     * Open stream.
     *
     * @param streamId stream id.
     * @param options  open stream options.
     * @return {@link Stream}.
     */
    CompletableFuture<Stream> openStream(long streamId, OpenStreamOptions options);
}
