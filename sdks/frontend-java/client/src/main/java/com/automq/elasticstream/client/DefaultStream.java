package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.AppendResult;
import com.automq.elasticstream.client.api.FetchResult;
import com.automq.elasticstream.client.api.RecordBatch;
import com.automq.elasticstream.client.api.RecordBatchWithContext;
import com.automq.elasticstream.client.api.Stream;

import io.netty.buffer.ByteBuf;

import java.nio.ByteBuffer;
import java.util.List;
import java.util.concurrent.CompletableFuture;

public class DefaultStream implements Stream {
    private final long streamId;
    private final com.automq.elasticstream.client.jni.Stream jniStream;

    public DefaultStream(long streamId, com.automq.elasticstream.client.jni.Stream jniStream) {
        this.streamId = streamId;
        this.jniStream = jniStream;
    }

    @Override
    public long streamId() {
        return streamId;
    }

    @Override
    public long startOffset() {
        try {
            return jniStream.startOffset().get();
        } catch (Throwable e) {
            throw new RuntimeException(e);
        }
    }

    @Override
    public long nextOffset() {
        try {
            return jniStream.nextOffset().get();
        } catch (Throwable e) {
            throw new RuntimeException(e);
        }
    }

    @Override
    public CompletableFuture<AppendResult> append(RecordBatch recordBatch) {
        ByteBuf buf = FlatRecordBatchCodec.encode(streamId, recordBatch);
        CompletableFuture<AppendResult> cf = jniStream.append(buf.nioBuffer()).thenApply(DefaultAppendResult::new);
        cf.whenComplete((rst, ex) -> {
            buf.release();
        });
        return cf;
    }

    @Override
    public CompletableFuture<FetchResult> fetch(long startOffset, long endOffset, int maxBytesHint) {
        return jniStream.read(startOffset, endOffset, maxBytesHint).thenApply(bytes -> {
            List<RecordBatchWithContext> records = FlatRecordBatchCodec.decode(ByteBuffer.wrap(bytes));
            return new DefaultFetchResult(records);
        });
    }

    @Override
    public CompletableFuture<Void> trim(long newStartOffset) {
        // TODO:
        return CompletableFuture.completedFuture(null);
    }

    @Override
    public CompletableFuture<Void> close() {
        return jniStream.asyncClose();
    }

    @Override
    public CompletableFuture<Void> destroy() {
        throw new UnsupportedOperationException();
    }
}
