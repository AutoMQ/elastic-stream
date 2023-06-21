package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.FetchResult;
import com.automq.elasticstream.client.api.RecordBatchWithContext;
import com.automq.elasticstream.client.jni.Frontend;
import com.automq.elasticstream.client.utils.BytesUtils;

import java.nio.ByteBuffer;
import java.util.List;

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
        Frontend.freeMemory(BytesUtils.getAddress(buffer), buffer.capacity());
    }
}
