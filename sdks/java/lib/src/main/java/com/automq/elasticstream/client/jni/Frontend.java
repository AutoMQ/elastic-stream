package com.automq.elasticstream.client.jni;
import java.util.concurrent.CompletableFuture;
public class Frontend extends ElasticStreamObject {
    public Frontend(String access_point) {
        this.ptr = getFrontend(access_point);
    }
    public CompletableFuture<Long> create(int replica, int ack, long retention_millis) {
        CompletableFuture<Long> future = new CompletableFuture<>(); 
        create(this.ptr, replica, ack, retention_millis, future);
        return future;
    }
    public CompletableFuture<Stream> open(long id, long epoch) {
        CompletableFuture<Stream> future = new CompletableFuture<>();
        open(this.ptr, id, epoch, future);
        return future;
    }
    private native void create(long ptr, int replica, int ack, long retention_millis, CompletableFuture<Long> future);
    private native void open(long ptr, long id, long epoch, CompletableFuture<Stream> future);
    private native long getFrontend(String access_point);
    private native void freeFrontend(long ptr);
    @Override
    public void close() {
        freeFrontend(this.ptr);
    }
}
