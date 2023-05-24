package com.automq.elasticstream.client.utils;

import com.google.common.util.concurrent.FutureCallback;
import com.google.common.util.concurrent.Futures;
import com.google.common.util.concurrent.ListenableFuture;
import com.google.common.util.concurrent.MoreExecutors;
import java.util.concurrent.CompletableFuture;

public class FutureUtils {
    public static <T> CompletableFuture<T> buildCompletableFuture(
        ListenableFuture<T> listenableFuture) {
        CompletableFuture<T> completableFuture = new CompletableFuture<T>() {
            @Override
            public boolean cancel(boolean mayInterruptIfRunning) {
                // propagate cancel to the listenable future
                boolean result = listenableFuture.cancel(mayInterruptIfRunning);
                super.cancel(mayInterruptIfRunning);
                return result;
            }
        };

        Futures.addCallback(listenableFuture, new FutureCallback<T>() {
            @Override
            public void onSuccess(T result) {
                completableFuture.complete(result);
            }

            @Override
            public void onFailure(Throwable t) {
                completableFuture.completeExceptionally(t);
            }
        }, MoreExecutors.directExecutor());
        return completableFuture;
    }
}
