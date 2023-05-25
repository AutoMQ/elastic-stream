package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.AppendResult;

public class DefaultAppendResult implements AppendResult {
    private final long baseOffset;

    public DefaultAppendResult(long baseOffset) {
        this.baseOffset = baseOffset;
    }
    @Override
    public long baseOffset() {
        return baseOffset;
    }

    public String toString() {
        return "AppendResult(baseOffset=" + baseOffset + ")";
    }
}
