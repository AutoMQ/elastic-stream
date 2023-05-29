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

        for (int i = 0; i < 10; i++) {
            byte[] payload = String.format("hello world %03d", i).getBytes(StandardCharsets.UTF_8);
            ByteBuffer buffer = ByteBuffer.wrap(payload);
            buffer.flip();
            AppendResult appendResult = stream
                    .append(new DefaultRecordBatch(10, 0, Collections.emptyMap(), buffer)).get();
            long offset = appendResult.baseOffset();
            System.out.println("append record result offset:" + offset);
            assert offset == i * 10;
        }

        for (int i = 0; i < 10; i++) {
            FetchResult fetchResult = stream.fetch(i * 10 , i * 10 + 10, 1).get();
            assert 1 == fetchResult.recordBatchList().size();
            RecordBatchWithContext recordBatch = fetchResult.recordBatchList().get(0);
            assert (i * 10) == recordBatch.baseOffset();
            assert (i * 10 + 9) == recordBatch.lastOffset();

            byte[] rawPayload = new byte[recordBatch.rawPayload().remaining()];
            recordBatch.rawPayload().get(rawPayload);
            String payloadStr = new String(rawPayload, StandardCharsets.UTF_8);
            System.out.println("fetch record result offset[" + recordBatch.baseOffset() + ","
                + recordBatch.lastOffset() + "]" + " payload:" + payloadStr + ".");
            assert String.format("hello world %03d", i).equals(payloadStr);
        }

    }

}
