package sdk.elastic.stream.integration.client.impl;

import java.time.Duration;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ExecutionException;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import sdk.elastic.stream.apis.ClientConfigurationBuilder;
import sdk.elastic.stream.apis.OperationClient;
import sdk.elastic.stream.client.impl.OperationClientBuilderImpl;
import sdk.elastic.stream.client.route.Address;
import sdk.elastic.stream.flatc.header.AppendResultT;
import sdk.elastic.stream.flatc.header.CreateStreamResultT;
import sdk.elastic.stream.flatc.header.ErrorCode;
import sdk.elastic.stream.flatc.header.StreamT;
import sdk.elastic.stream.models.RecordBatch;
import sdk.elastic.stream.models.RecordBatchesGenerator;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

class OperationClientImplTest {
    private static final Address defaultPmAddress = Address.fromAddress(System.getProperty("pm.address"));
    private static final Duration DEFAULT_REQUEST_TIMEOUT = Duration.ofSeconds(8);
    private static OperationClient operationClient;

    @BeforeAll
    public static void setUp() throws Exception {
        ClientConfigurationBuilder builder = new ClientConfigurationBuilder()
            .setPmEndpoint(String.format("%s:%d", defaultPmAddress.getHost(), defaultPmAddress.getPort()))
            .setConnectionTimeout(Duration.ofSeconds(3))
            .setChannelMaxIdleTime(Duration.ofSeconds(10))
            .setHeartBeatInterval(Duration.ofSeconds(9));
        operationClient = new OperationClientBuilderImpl().setClientConfiguration(builder.build()).build();
        operationClient.start();
    }

    @Test
    void testAppendAndFetchBatch() throws ExecutionException, InterruptedException {
        StreamT resultStream = createOneStream().getStream();
        long streamId = resultStream.getStreamId();

        Long startOffset = operationClient.getLastWritableOffset(streamId, DEFAULT_REQUEST_TIMEOUT).get();
        assertEquals(startOffset, 0L);

        RecordBatch batch = RecordBatchesGenerator.generateOneRecordBatch(streamId);
        AppendResultT appendResultT = operationClient.appendBatch(batch, DEFAULT_REQUEST_TIMEOUT).get();

        assertEquals(appendResultT.getStatus().getCode(), ErrorCode.OK);
        assertEquals(appendResultT.getStreamId(), streamId);
        long baseOffset = appendResultT.getBaseOffset();

        List<RecordBatch> recordBatches = operationClient.fetchBatches(streamId, baseOffset, 1, Integer.MAX_VALUE, DEFAULT_REQUEST_TIMEOUT).get();
        assertEquals(1L, recordBatches.size());
        assertTrue(batch.recordsEquivalent(recordBatches.get(0)));

    }

    @Test
    void testAppendAndFetchMultipleBatches() throws ExecutionException, InterruptedException {
        StreamT resultStream = createOneStream().getStream();
        long streamId = resultStream.getStreamId();

        // append 3 batches one by one.
        int batchCount = 3;
        RecordBatch batch = RecordBatchesGenerator.generateOneRecordBatch(streamId);
        int recordsPerBatch = batch.getRecords().size();

        for (int i = 0; i < batchCount; i++) {
            operationClient.appendBatch(batch, DEFAULT_REQUEST_TIMEOUT).get();
        }
        int totalRecords = recordsPerBatch * batchCount;

        Long lastOffset = operationClient.getLastWritableOffset(streamId, DEFAULT_REQUEST_TIMEOUT).get();
        assertEquals(totalRecords, lastOffset);

        // fetch from the second batch.
        List<RecordBatch> recordBatches = operationClient.fetchBatches(streamId, recordsPerBatch, 1, Integer.MAX_VALUE, DEFAULT_REQUEST_TIMEOUT).get();
        assertEquals(batchCount - 1, recordBatches.size());
        assertTrue(batch.recordsEquivalent(recordBatches.get(recordBatches.size()-1)));
    }

    @Test
    void heartbeat() throws ExecutionException, InterruptedException {
        Boolean result = operationClient.heartbeat(defaultPmAddress, DEFAULT_REQUEST_TIMEOUT).get();
        assertTrue(result);
    }

    private CreateStreamResultT createOneStream() throws ExecutionException, InterruptedException {
        StreamT streamT = new StreamT();
        streamT.setStreamId(0L);
        streamT.setReplicaNum((byte) 1);
        streamT.setRetentionPeriodMs(Duration.ofDays(3).toMillis());
        List<StreamT> streamTList = new ArrayList<>(1);
        streamTList.add(streamT);
        return operationClient.createStreams(streamTList, DEFAULT_REQUEST_TIMEOUT)
            .get()
            .get(0);
    }

    @AfterAll
    public static void tearDown() throws Exception {
        operationClient.close();
    }
}