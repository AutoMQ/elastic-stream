package elastic.stream.benchmark.tool;

import lombok.SneakyThrows;
import sdk.elastic.stream.apis.ClientConfiguration;
import sdk.elastic.stream.apis.ClientConfigurationBuilder;
import sdk.elastic.stream.apis.OperationClient;
import sdk.elastic.stream.client.impl.OperationClientBuilderImpl;
import sdk.elastic.stream.flatc.header.AppendResultT;
import sdk.elastic.stream.flatc.header.CreateStreamResultT;
import sdk.elastic.stream.flatc.header.ErrorCode;
import sdk.elastic.stream.flatc.header.StreamT;
import sdk.elastic.stream.models.Record;
import sdk.elastic.stream.models.RecordBatch;

import java.nio.ByteBuffer;
import java.time.Duration;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.IntStream;

/**
 * @author ningyu
 */
public class ClientTool {

    private static final Duration DEFAULT_REQUEST_TIMEOUT = Duration.ofSeconds(10);

    private static final int CREATE_STREAM_BATCH_SIZE = 10;

    @SneakyThrows
    public static OperationClient buildClient(String pmAddress, int streamCacheSize, int semaphore) {
        ClientConfiguration configuration = new ClientConfigurationBuilder()
                .setPmEndpoint(pmAddress)
                .setStreamCacheSize(streamCacheSize)
                .setClientAsyncSemaphoreValue(semaphore)
                .build();
        OperationClient client = new OperationClientBuilderImpl().setClientConfiguration(configuration).build();
        client.start();
        return client;
    }

    @SneakyThrows
    public static List<Long> createStreams(OperationClient client, int streamCount) {
        List<Long> streamIds = new ArrayList<>();
        for (int i = 0; i < streamCount; i += CREATE_STREAM_BATCH_SIZE) {
            int toIndex = Math.min(i + CREATE_STREAM_BATCH_SIZE, streamCount);
            List<StreamT> streams = IntStream.range(i, toIndex).mapToObj(j -> {
                StreamT streamT = new StreamT();
                streamT.setStreamId(0L);
                streamT.setReplicaNum((byte) 1);
                streamT.setRetentionPeriodMs(Duration.ofDays(3).toMillis());
                return streamT;
            }).toList();
            List<CreateStreamResultT> resultList = client.createStreams(streams, DEFAULT_REQUEST_TIMEOUT).get();
            List<Long> batchStreamIds = resultList.stream()
                    .map(CreateStreamResultT::getStream)
                    .map(StreamT::getStreamId)
                    .toList();
            streamIds.addAll(batchStreamIds);
        }
        return streamIds;
    }

    @SneakyThrows
    public static boolean fetchOne(OperationClient client, long streamId, long offset, int expectedBodySize) {
        List<RecordBatch> batches = client.fetchBatches(streamId, offset, 1, 1, DEFAULT_REQUEST_TIMEOUT).get();
        return checkBatches(batches, expectedBodySize);
    }

    @SneakyThrows
    public static Long fetchFrom(OperationClient client, long streamId, long offset, int expectedBodySize) {
        List<RecordBatch> batches = client.fetchBatches(streamId, offset, 1, Integer.MAX_VALUE, DEFAULT_REQUEST_TIMEOUT).get();
        if (!checkBatches(batches, expectedBodySize)) {
            return null;
        }
        return batches.stream()
                .flatMap(batch -> batch.getRecords().stream())
                .map(Record::getOffset)
                .max(Long::compareTo)
                .orElse(null);
    }

    private static boolean checkBatches(List<RecordBatch> batches, int expectedBodySize) {
        if (batches.isEmpty()) {
            return false;
        }
        for (RecordBatch batch : batches) {
            if (batch.getRecords().isEmpty()) {
                return false;
            }
            for (Record record : batch.getRecords()) {
                if (record.getBody() == null || record.getBody().remaining() != expectedBodySize) {
                    return false;
                }
            }
        }
        return true;
    }

    @SneakyThrows
    public static long append(OperationClient client, long streamId, byte[] payload) {
        List<Record> recordList = List.of(new Record(streamId, 0L, 42L, null, null, ByteBuffer.wrap(payload)));
        AppendResultT resultT = client.appendBatch(new RecordBatch(streamId, null, recordList), DEFAULT_REQUEST_TIMEOUT).get();
        if (resultT.getStatus().getCode() != ErrorCode.OK) {
            throw new RuntimeException("failed to append a batch to stream " + streamId + ", code: " + resultT.getStatus().getCode() + ", message: " + resultT.getStatus().getMessage());
        }
        return resultT.getBaseOffset();
    }
}
