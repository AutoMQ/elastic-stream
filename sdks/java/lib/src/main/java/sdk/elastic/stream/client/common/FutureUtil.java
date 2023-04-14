package sdk.elastic.stream.client.common;

import java.util.concurrent.CompletableFuture;

public class FutureUtil {
    /**
     * Returns a new CompletableFuture that is already completed with the given exception.
     *
     * @param ex  the exception
     * @param <U> the type of the returned CompletableFuture's result
     * @return the new CompletableFuture
     */
    public static <U> CompletableFuture<U> failedFuture(Throwable ex) {
        CompletableFuture<U> future = new CompletableFuture<>();
        future.completeExceptionally(ex);
        return future;
    }
}
