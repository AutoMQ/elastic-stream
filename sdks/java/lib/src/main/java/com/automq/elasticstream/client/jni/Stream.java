package com.automq.elasticstream.client.jni;
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
    public CompletableFuture<Long> append(byte[] data, int count) {
        CompletableFuture<Long> future = new CompletableFuture<>();
        append(this.ptr, data, count, future);
        return future;
    }
    public CompletableFuture<byte[]> read(long start_offset, long end_offset, int batch_max_bytes) {
        CompletableFuture<byte[]> future = new CompletableFuture<>();
        read(this.ptr, start_offset, end_offset, batch_max_bytes, future);
        return future;
    }
    private native void startOffset(long ptr, CompletableFuture<Long> future);
    private native void nextOffset(long ptr, CompletableFuture<Long> future);
    private native void append(long ptr, byte[] data, int count, CompletableFuture<Long> future);
    private native void read(long ptr, long start_offset, long end_offset, int batch_max_bytes, CompletableFuture<byte[]> future);
    private native long freeStream(long ptr);
    @Override
    public void close() {
        freeStream(this.ptr);
    }
    
}
