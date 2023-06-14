package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.FetchResult;
import com.automq.elasticstream.client.api.RecordBatchWithContext;
import com.automq.elasticstream.client.jni.Frontend;

import java.nio.ByteBuffer;
import java.util.List;

import sun.nio.ch.DirectBuffer;

public class DefaultFetchResult implements FetchResult {
    private final ByteBuffer buffer;
    private final List<RecordBatchWithContext> records;

    public DefaultFetchResult(ByteBuffer buffer, List<RecordBatchWithContext> records) {
        this.buffer = buffer;
        this.records = records;
    }

    @Override
    public List<RecordBatchWithContext> recordBatchList() {
        return records;
    }

    @Override
    public void free() {
        Frontend.freeMemory(((DirectBuffer) buffer).address(), buffer.capacity());
    }
}
