package elastic.stream.benchmark.jmh;

import lombok.SneakyThrows;
import org.openjdk.jmh.annotations.Benchmark;
import org.openjdk.jmh.annotations.Group;
import org.openjdk.jmh.annotations.Param;
import org.openjdk.jmh.annotations.Scope;
import org.openjdk.jmh.annotations.Setup;
import org.openjdk.jmh.annotations.State;
import org.openjdk.jmh.annotations.TearDown;
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
import java.security.SecureRandom;
import java.time.Duration;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Collectors;
import java.util.stream.IntStream;

/**
 * @author ningyu
 */
@State(Scope.Group)
public class ReadWrite {

    private static final Duration DEFAULT_REQUEST_TIMEOUT = Duration.ofSeconds(60);
    private static final int PRE_APPEND_RECORD_COUNT = 10;

    @Param({"localhost:2378"})
    private String pmAddress;
    @Param({"1024"})
    private int bodySize;

    @Param({"64"})
    private int streamCount;

    private OperationClient client;
    private byte[] payload;
    private List<Long> streamIds;
    private List<Long> baseOffsets;

    @Setup
    public void setup() {
        this.client = buildClient(pmAddress);
        this.streamIds = createStreams(client, streamCount);
        this.payload = randomPayload(bodySize);
        this.baseOffsets = prepareRecords(client, streamIds.get(0), payload);
    }

    @TearDown
    @SneakyThrows
    public void tearDown() {
        client.close();
    }

    @SneakyThrows
    private OperationClient buildClient(String pmAddress) {
        ClientConfigurationBuilder builder = new ClientConfigurationBuilder()
                .setPmEndpoint(pmAddress)
                // TODO make these options configurable
                .setClientAsyncSemaphoreValue(1024)
                .setConnectionTimeout(Duration.ofSeconds(3))
                .setChannelMaxIdleTime(Duration.ofSeconds(10))
                .setHeartBeatInterval(Duration.ofSeconds(5));
        OperationClient client = new OperationClientBuilderImpl().setClientConfiguration(builder.build()).build();
        client.start();
        return client;
    }

    private List<Long> createStreams(OperationClient client, int streamCount) {
        return IntStream.range(0, streamCount).mapToObj(i -> createStream(client)).collect(Collectors.toList());
    }

    @SneakyThrows
    private long createStream(OperationClient client) {
        StreamT streamT = new StreamT();
        streamT.setStreamId(0L);
        streamT.setReplicaNums((byte) 1);
        streamT.setRetentionPeriodMs(Duration.ofDays(3).toMillis());
        List<StreamT> streamList = new ArrayList<>(1);
        streamList.add(streamT);
        List<CreateStreamResultT> resultList = client.createStreams(streamList, DEFAULT_REQUEST_TIMEOUT).get();
        return resultList.get(0).getStream().getStreamId();
    }

    @SneakyThrows
    private byte[] randomPayload(int size) {
        byte[] payload = new byte[size];
        SecureRandom.getInstanceStrong().nextBytes(payload);
        return payload;
    }

    private List<Long> prepareRecords(OperationClient client, long streamId, byte[] payload) {
        List<Long> baseOffsets = new ArrayList<>();
        for (int i = 0; i < PRE_APPEND_RECORD_COUNT; i++) {
            baseOffsets.add(append(client, streamId, payload));
        }
        return baseOffsets;
    }

    @Benchmark
    @Group("readWrite")
    public void read() {
        fetch(client, streamIds.get(0), baseOffsets.get(new SecureRandom().nextInt(PRE_APPEND_RECORD_COUNT)));
    }

    @Benchmark
    @Group("readWrite")
    public void write() {
        append(client, streamIds.get(new SecureRandom().nextInt(streamCount)), payload);
    }

    @SneakyThrows
    private void fetch(OperationClient client, long streamId, long offset) {
        List<RecordBatch> batches = client.fetchBatches(streamId, offset, 1, 1, DEFAULT_REQUEST_TIMEOUT).get();
        if (batches.isEmpty()) {
            throw new RuntimeException("failed to fetch a batch from stream " + streamId + " at offset " + offset + ": empty batches");
        }
        batches.forEach(batch -> {
            if (batch.getRecords().isEmpty()) {
                throw new RuntimeException("failed to fetch a batch from stream " + streamId + " at offset " + offset + ": empty records");
            }
            batch.getRecords().forEach(record -> {
                if (record.getBody() == null || record.getBody().remaining() != bodySize) {
                    throw new RuntimeException("failed to fetch a batch from stream " + streamId + " at offset " + offset + ": body size mismatch, expected: " + bodySize + ", actual: " + record.getBody().remaining());
                }
            });
        });
    }

    @SneakyThrows
    private long append(OperationClient client, long streamId, byte[] payload) {
        List<Record> recordList = List.of(new Record(streamId, 0L, 42L, null, null, ByteBuffer.wrap(payload)));
        AppendResultT resultT = client.appendBatch(new RecordBatch(streamId, null, recordList), DEFAULT_REQUEST_TIMEOUT).get();
        if (resultT.getStatus().getCode() != ErrorCode.OK) {
            throw new RuntimeException("failed to append a batch to stream " + streamId + ", code: " + resultT.getStatus().getCode() + ", message: " + resultT.getStatus().getMessage());
        }
        return resultT.getBaseOffset();
    }
}