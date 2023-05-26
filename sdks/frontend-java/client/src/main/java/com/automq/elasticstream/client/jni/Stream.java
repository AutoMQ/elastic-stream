package com.automq.elasticstream.client.jni;
import java.nio.ByteBuffer;
import java.util.concurrent.CompletableFuture;
public class Stream extends ElasticStreamObject {
    public Stream(long ptr) {
        this.ptr = ptr;
    }
    public CompletableFuture<Long> startOffset() {
        CompletableFuture<Long> future = new CompletableFuture<>();
        startOffset(this.ptr, future);
        return future;
    }
    public CompletableFuture<Long> nextOffset() {
        CompletableFuture<Long> future = new CompletableFuture<>();
        nextOffset(this.ptr, future);
        return future;
    }
    public CompletableFuture<Long> append(ByteBuffer data) {
        CompletableFuture<Long> future = new CompletableFuture<>();
        append(this.ptr, data, future);
        return future;
    }
    public CompletableFuture<ByteBuffer> read(long offset, int limit, int max_bytes) {
        CompletableFuture<ByteBuffer> future = new CompletableFuture<>();
        read(this.ptr, offset, limit, max_bytes, future);
        return future;
    }
    private native void startOffset(long ptr, CompletableFuture<Long> future);
    private native void nextOffset(long ptr, CompletableFuture<Long> future);
    private native void append(long ptr, ByteBuffer data, CompletableFuture<Long> future);
    private native void read(long ptr, long offset, int limit, int max_bytes, CompletableFuture<ByteBuffer> future);
    private native long freeStream(long ptr);
    @Override
    public void close() {
        freeStream(this.ptr);
    }
    
}
