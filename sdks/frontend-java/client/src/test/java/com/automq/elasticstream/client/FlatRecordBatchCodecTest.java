package com.automq.elasticstream.client;

import com.automq.elasticstream.client.api.RecordBatch;
import com.automq.elasticstream.client.api.RecordBatchWithContext;
import org.junit.Assert;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.junit.runners.JUnit4;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

@RunWith(JUnit4.class)
public class FlatRecordBatchCodecTest {

    @Test
    public void testCodec() {
        byte[] payload = "hello world".getBytes(StandardCharsets.UTF_8);
        ByteBuffer buffer = ByteBuffer.wrap(payload);
        buffer.flip();
        Map<String, String> properties = new HashMap<>();
        properties.put("k1", "v1");
        properties.put("k2", "v2");
        RecordBatch src = new DefaultRecordBatch(payload.length, 233, properties, buffer);
        List<RecordBatchWithContext> list = FlatRecordBatchCodec.decode(FlatRecordBatchCodec.encode(123, src));
        Assert.assertEquals(1, list.size());
        RecordBatchWithContext dst = list.get(0);
        Assert.assertEquals(src.count(), dst.count());
        Assert.assertEquals(src.baseTimestamp(), dst.baseTimestamp());
        Assert.assertEquals(0, dst.baseOffset());
        Assert.assertEquals(src.count(), dst.lastOffset());
        Assert.assertEquals(properties, dst.properties());
        Assert.assertEquals(buffer, dst.rawPayload());
    }


}
