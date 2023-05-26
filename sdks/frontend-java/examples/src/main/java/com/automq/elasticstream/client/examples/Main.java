package com.automq.elasticstream.client.examples;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Collections;

import com.automq.elasticstream.client.DefaultRecordBatch;
import com.automq.elasticstream.client.api.AppendResult;
import com.automq.elasticstream.client.api.Client;
import com.automq.elasticstream.client.api.CreateStreamOptions;
import com.automq.elasticstream.client.api.FetchResult;
import com.automq.elasticstream.client.api.RecordBatchWithContext;
import com.automq.elasticstream.client.api.Stream;

public class Main {

    public static void main(String[] args) throws Exception {
        Client client = Client.builder().endpoint("127.0.0.1:12378").build();
        Stream stream = client.streamClient()
                .createAndOpenStream(CreateStreamOptions.newBuilder().replicaCount(1).build()).get();

        byte[] payload = "hello world".getBytes(StandardCharsets.UTF_8);
        ByteBuffer buffer = ByteBuffer.wrap(payload);
        buffer.flip();
        AppendResult appendResult = stream
                .append(new DefaultRecordBatch(payload.length, 0, Collections.emptyMap(), buffer)).get();
        long offset = appendResult.baseOffset();
        System.out.println("append record result offset:" + offset);

        FetchResult fetchResult = stream.fetch(offset, offset + 1, 1).get();
        for (RecordBatchWithContext recordBatchWithContext : fetchResult.recordBatchList()) {
            byte[] rawPayload = new byte[recordBatchWithContext.rawPayload().remaining()];
            recordBatchWithContext.rawPayload().get(rawPayload);
            System.out.println("fetch record result offset[" + recordBatchWithContext.baseOffset() + ","
                    + recordBatchWithContext.lastOffset() + "]" + " payload:"
                    + new String(rawPayload, StandardCharsets.UTF_8));
        }
    }

}
