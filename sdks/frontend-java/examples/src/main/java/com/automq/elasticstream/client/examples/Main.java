package com.automq.elasticstream.client.examples;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.sql.Time;
import java.util.Collections;
import java.util.concurrent.TimeUnit;

import com.automq.elasticstream.client.DefaultRecordBatch;
import com.automq.elasticstream.client.api.AppendResult;
import com.automq.elasticstream.client.api.Client;
import com.automq.elasticstream.client.api.CreateStreamOptions;
import com.automq.elasticstream.client.api.FetchResult;
import com.automq.elasticstream.client.api.OpenStreamOptions;
import com.automq.elasticstream.client.api.RecordBatchWithContext;
import com.automq.elasticstream.client.api.Stream;
import static org.junit.Assert.*;


public class Main {

    public static void main(String[] args) throws Exception {
        Client client = Client.builder().endpoint("127.0.0.1:12378").kvEndpoint("127.0.0.1:12379").build();
        Stream stream = client.streamClient()
                .createAndOpenStream(CreateStreamOptions.newBuilder().replicaCount(1).build()).get();
        long streamId = stream.streamId();

        System.out.println("Step1: append 10 records to stream:" + streamId);
        for (int i = 0; i < 10; i++) {
            byte[] payload = String.format("hello world %03d", i).getBytes(StandardCharsets.UTF_8);
            ByteBuffer buffer = ByteBuffer.wrap(payload);
            AppendResult appendResult = stream
                    .append(new DefaultRecordBatch(10, 0, Collections.emptyMap(), buffer)).get();
            long offset = appendResult.baseOffset();
            System.out.println("append record result offset:" + offset);
            assertEquals(i * 10, offset);
        }

        System.out.println("Step2: read 10 record batch one by one");
        for (int i = 0; i < 10; i++) {
            FetchResult fetchResult = stream.fetch(i * 10, i * 10 + 10, Integer.MAX_VALUE).get();
            assertEquals(1, fetchResult.recordBatchList().size());
            RecordBatchWithContext recordBatch = fetchResult.recordBatchList().get(0);
            assertEquals(i * 10, recordBatch.baseOffset());
            assertEquals(i * 10 + 10, recordBatch.lastOffset());

            byte[] rawPayload = new byte[recordBatch.rawPayload().remaining()];
            recordBatch.rawPayload().get(rawPayload);
            String payloadStr = new String(rawPayload, StandardCharsets.UTF_8);
            System.out.println("fetch record result offset[" + recordBatch.baseOffset() + ","
                + recordBatch.lastOffset() + "]" + " payload:" + payloadStr + ".");
            assertEquals(String.format("hello world %03d", i), payloadStr);
        }

        System.out.println("Step3: cross read 10 record batch");
        for (int i = 0; i < 9; i++) {
            FetchResult fetchResult = stream.fetch(i * 10, i * 10 + 11, Integer.MAX_VALUE).get();
            assertEquals(2, fetchResult.recordBatchList().size());
            for (int j = 0; j < 2; j++) {
                int index = i + j;
                RecordBatchWithContext recordBatch = fetchResult.recordBatchList().get(j);
                assertEquals(index * 10, recordBatch.baseOffset());
                assertEquals(index * 10 + 10, recordBatch.lastOffset());
                byte[] rawPayload = new byte[recordBatch.rawPayload().remaining()];
                recordBatch.rawPayload().get(rawPayload);
                String payloadStr = new String(rawPayload, StandardCharsets.UTF_8);
                assertEquals(String.format("hello world %03d", index), payloadStr);
                System.out.println("fetch record result offset[" + recordBatch.baseOffset() + ","
                    + recordBatch.lastOffset() + "]" + " payload:" + payloadStr + ".");
            }
        }

        System.out.println("Step4: reopen stream");
        stream.close().get();
        stream = client.streamClient().openStream(streamId, OpenStreamOptions.newBuilder().build()).get();

        System.out.println("Step5: append more 10 record batches");
        for (int i = 0; i < 10; i++) {
            int index = i + 10;
            byte[] payload = String.format("hello world %03d", index).getBytes(StandardCharsets.UTF_8);
            ByteBuffer buffer = ByteBuffer.wrap(payload);
            AppendResult appendResult = stream
                    .append(new DefaultRecordBatch(10, 0, Collections.emptyMap(), buffer)).get();
            long offset = appendResult.baseOffset();
            System.out.println("append record result offset:" + offset);
            assertEquals(index * 10, offset);
        }

        System.out.println("Step6: read 20 record batch one by one");
        for (int i = 0; i < 20; i++) {
            FetchResult fetchResult = stream.fetch(i * 10 , i * 10 + 10, Integer.MAX_VALUE).get();
            assertEquals(1, fetchResult.recordBatchList().size());
            RecordBatchWithContext recordBatch = fetchResult.recordBatchList().get(0);
            assertEquals(i * 10, recordBatch.baseOffset());
            assertEquals(i * 10 + 10, recordBatch.lastOffset());

            byte[] rawPayload = new byte[recordBatch.rawPayload().remaining()];
            recordBatch.rawPayload().get(rawPayload);
            String payloadStr = new String(rawPayload, StandardCharsets.UTF_8);
            System.out.println("fetch record result offset[" + recordBatch.baseOffset() + ","
                + recordBatch.lastOffset() + "]" + " payload:" + payloadStr + ".");
            assertEquals(String.format("hello world %03d", i), payloadStr);
        }

        while(true) {
            TimeUnit.SECONDS.sleep(1);
            System.out.println("Tick");
        }
    }

}
