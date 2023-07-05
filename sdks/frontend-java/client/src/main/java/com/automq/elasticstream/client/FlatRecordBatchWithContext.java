package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.RecordBatchWithContext;
import com.automq.elasticstream.client.flatc.records.KeyValueT;
import com.automq.elasticstream.client.flatc.records.RecordBatchMetaT;

import java.nio.ByteBuffer;
import java.util.Arrays;
import java.util.Collections;
import java.util.Map;
import java.util.stream.Collectors;

public class FlatRecordBatchWithContext implements RecordBatchWithContext {
    private final RecordBatchMetaT meta;
    private final ByteBuffer payload;
    private final Map<String, String> properties;

    public FlatRecordBatchWithContext(RecordBatchMetaT meta, ByteBuffer payload) {
        this.meta = meta;
        this.payload = payload;
        this.properties = meta.getProperties().length == 0
                ? Collections.emptyMap()
                : Arrays.stream(meta.getProperties()).collect(Collectors.toMap(KeyValueT::getKey, KeyValueT::getValue));
    }

    @Override
    public int count() {
        return meta.getLastOffsetDelta();
    }

    @Override
    public long baseTimestamp() {
        return meta.getBaseTimestamp();
    }

    @Override
    public Map<String, String> properties() {
        return properties;
    }

    @Override
    public ByteBuffer rawPayload() {
        return payload.duplicate();
    }

    @Override
    public long baseOffset() {
        return meta.getBaseOffset();
    }

    @Override
    public long lastOffset() {
        if (meta.getBaseOffset() >= 0) {
            return meta.getBaseOffset() + meta.getLastOffsetDelta();
        }
        return meta.getLastOffsetDelta();
    }

}
