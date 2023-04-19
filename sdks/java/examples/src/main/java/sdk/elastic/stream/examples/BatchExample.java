package sdk.elastic.stream.examples;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ExecutionException;
import sdk.elastic.stream.apis.ClientConfigurationBuilder;
import sdk.elastic.stream.apis.OperationClient;
import sdk.elastic.stream.client.impl.OperationClientBuilderImpl;
import sdk.elastic.stream.flatc.header.AppendResultT;
import sdk.elastic.stream.flatc.header.CreateStreamResultT;
import sdk.elastic.stream.flatc.header.ErrorCode;
import sdk.elastic.stream.flatc.header.StreamT;
import sdk.elastic.stream.models.Headers;
import sdk.elastic.stream.models.Record;
import sdk.elastic.stream.models.RecordBatch;

import static sdk.elastic.stream.models.HeaderKey.CreatedAt;
import static sdk.elastic.stream.models.HeaderKey.Tag;

public class BatchExample {
    private static final Duration DEFAULT_REQUEST_TIMEOUT = Duration.ofSeconds(8);
    private static OperationClient operationClient;

    public static void init(String pmAddress) throws Exception {
        ClientConfigurationBuilder builder = new ClientConfigurationBuilder()
            .setPmEndpoint(pmAddress)
            .setConnectionTimeout(Duration.ofSeconds(3))
            .setChannelMaxIdleTime(Duration.ofSeconds(10))
            .setHeartBeatInterval(Duration.ofSeconds(9));
        operationClient = new OperationClientBuilderImpl().setClientConfiguration(builder.build()).build();
        operationClient.start();
    }

    public static StreamT createOneStream() throws ExecutionException, InterruptedException {
        StreamT streamT = new StreamT();
        streamT.setStreamId(0L);
        streamT.setReplicaNum((byte) 1);
        streamT.setRetentionPeriodMs(Duration.ofDays(3).toMillis());
        List<StreamT> streamTList = new ArrayList<>(1);
        streamTList.add(streamT);
        List<CreateStreamResultT> resultList = operationClient.createStreams(streamTList, DEFAULT_REQUEST_TIMEOUT).get();
        return resultList.get(0).getStream();
    }

    public static long getLastWritableOffset(long streamId) throws ExecutionException, InterruptedException {
        return operationClient.getLastWritableOffset(streamId, DEFAULT_REQUEST_TIMEOUT).get();
    }

    public static long appendOneSampleBatch(long streamId) throws ExecutionException, InterruptedException {
        List<Record> recordList = generateRecordList(streamId, 0L);
        Map<String, String> batchProperties = new HashMap<>();
        batchProperties.put("batchPropertyA", "batchValueA");
        batchProperties.put("batchPropertyB", "batchValueB");
        AppendResultT resultT = operationClient.appendBatch(new RecordBatch(streamId, batchProperties, recordList), DEFAULT_REQUEST_TIMEOUT).get();
        if (resultT.getStatus().getCode() == ErrorCode.OK) {
            System.out.println("successfully append a batch to stream " + streamId + ", returned baseOffset is " + resultT.getBaseOffset());
        }

        return resultT.getBaseOffset();
    }

    public static void fetchBatches(long streamId, long baseOffset) throws ExecutionException, InterruptedException {
        System.out.println("starting to fetch batches from stream " + streamId + ", baseOffset " + baseOffset);
        List<RecordBatch> batches = operationClient.fetchBatches(streamId, baseOffset, 1, Integer.MAX_VALUE, DEFAULT_REQUEST_TIMEOUT).get();

        for (int i = 0; i < batches.size(); i++) {
            System.out.println("==== Info for Batch " + (i + 1) + ", Stream " + streamId + " ====");
            System.out.println("Batch's baseOffset is: " + batches.get(i).getBaseOffset());
            for (int j = 0; j < batches.get(i).getRecords().size(); j++) {
                System.out.println("====== Info for Record " + (j + 1) + ", Batch "+ (i + 1) + " ======");
                System.out.println("Offset is: " + batches.get(i).getRecords().get(j).getOffset());
                ByteBuffer body = batches.get(i).getRecords().get(j).getBody();
                byte[] bodyBytes = new byte[body.remaining()];
                body.get(bodyBytes);
                System.out.println("Message is: " + new String(bodyBytes, StandardCharsets.ISO_8859_1));
            }
        }
    }

    public static void main(String[] args) throws IOException {
        try {
            // pls input a valid pm address.
            init("localhost:8080");
            // 1. Create one Stream First.
            StreamT resultStream = createOneStream();
            long streamId = resultStream.getStreamId();

            // You may need the startOffset.
            long lastOffset = getLastWritableOffset(streamId);
            System.out.println("The last writable offset of Stream " + streamId + " is: " + lastOffset);

            // 2. Append one Batch to the targeted stream. Note that the storage server may change the batch's baseOffset.
            long returnedBaseOffset = appendOneSampleBatch(streamId);

            // 3. Fetch batches from some offset.
            // In this example, we fetch the batch that was appended in the last step.
            fetchBatches(streamId, returnedBaseOffset);

        } catch (Exception ex) {
            ex.printStackTrace();
        } finally {
            if (operationClient != null) {
                operationClient.close();
            }
        }
    }

    private static List<Record> generateRecordList(long streamId, long baseOffset) {
        List<Record> recordList = new ArrayList<>(2);

        Headers headers = new Headers();
        headers.addHeader(CreatedAt, "someTime");
        Map<String, String> properties = new HashMap<>();
        properties.put("propertyA", "valueA");
        recordList.add(new Record(streamId, baseOffset, 13L, headers, properties, ByteBuffer.wrap("Hello World1".getBytes(StandardCharsets.ISO_8859_1))));

        Headers headers2 = new Headers();
        headers2.addHeader(CreatedAt, "someTime2");
        headers2.addHeader(Tag, "onlyTest");
        Map<String, String> properties2 = new HashMap<>();
        properties2.put("propertyB", "valueB");
        properties2.put("propertyC", "valueC");
        recordList.add(new Record(streamId, baseOffset + 10, 11L, headers2, properties2, ByteBuffer.wrap("Hello World2".getBytes(StandardCharsets.ISO_8859_1))));

        return recordList;
    }

    public void close() throws IOException {
        operationClient.close();
    }
}
