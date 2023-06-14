package com.automq.elasticstream.client.jni;
import io.netty.channel.epoll.Native;
import io.netty.util.internal.NativeLibraryLoader;
import io.netty.util.internal.PlatformDependent;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import java.nio.ByteBuffer;
import java.util.concurrent.CompletableFuture;

public class Frontend extends ElasticStreamObject {

    private static final Logger LOGGER = LoggerFactory.getLogger(Frontend.class);

    static {
        loadNativeLibrary();
    }

    private static void loadNativeLibrary() {
        String sharedLibName = "frontend";
        ClassLoader cl = PlatformDependent.getClassLoader(Native.class);
        try {
            NativeLibraryLoader.load(sharedLibName, cl);
        } catch (UnsatisfiedLinkError e) {
            LOGGER.error("Failed to load shared library", e);
            throw e;
        }
    }

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

    public static native ByteBuffer allocateDirect(int size);

    public static native void freeMemory(long ptr, int size);

    @Override
    public void close() {
        freeFrontend(this.ptr);
    }
}
