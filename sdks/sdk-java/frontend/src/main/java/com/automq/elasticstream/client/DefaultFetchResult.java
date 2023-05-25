package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.FetchResult;
import com.automq.elasticstream.client.api.RecordBatchWithContext;

import java.util.List;

public class DefaultFetchResult implements FetchResult {
    private final List<RecordBatchWithContext> records;

    public DefaultFetchResult(List<RecordBatchWithContext> records) {
        this.records = records;
    }

    @Override
    public List<RecordBatchWithContext> recordBatchList() {
        return records;
    }
}
