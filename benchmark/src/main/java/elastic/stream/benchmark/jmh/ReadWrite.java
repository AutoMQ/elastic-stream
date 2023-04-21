package elastic.stream.benchmark.jmh;

import lombok.Getter;
import lombok.SneakyThrows;
import org.openjdk.jmh.annotations.Benchmark;
import org.openjdk.jmh.annotations.Group;
import org.openjdk.jmh.annotations.Level;
import org.openjdk.jmh.annotations.Param;
import org.openjdk.jmh.annotations.Scope;
import org.openjdk.jmh.annotations.Setup;
import org.openjdk.jmh.annotations.State;
import org.openjdk.jmh.annotations.TearDown;
import org.openjdk.jmh.infra.BenchmarkParams;
import sdk.elastic.stream.apis.OperationClient;

import java.security.SecureRandom;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Random;
import java.util.concurrent.ConcurrentHashMap;
import java.util.stream.IntStream;

import static elastic.stream.benchmark.tool.ClientTool.append;
import static elastic.stream.benchmark.tool.ClientTool.buildClient;
import static elastic.stream.benchmark.tool.ClientTool.createStreams;
import static elastic.stream.benchmark.tool.ClientTool.fetchFrom;
import static elastic.stream.benchmark.tool.ClientTool.fetchOne;

/**
 * @author ningyu
 */
@State(Scope.Thread)
public class ReadWrite {

    private static final int PRE_APPEND_RECORD_COUNT = 10;

    private final Random random = new SecureRandom();

    @State(Scope.Benchmark)
    @Getter
    public static class BenchmarkState {
        @Param({"localhost:2378"})
        private String pmAddress;
        @Param({"65536"})
        private int bodySize;
        @Param({"64"})
        private int streamCount;

        private byte[] payload;
        private List<Long> streamIds;
        private Map<Long, Long> lastOffsetsMap;

        @Setup
        public void setup(BenchmarkParams params) throws Exception {
            OperationClient client = buildClient(pmAddress, streamCount, 1024);
            this.streamIds = createStreams(client, streamCount);
            this.payload = randomPayload(bodySize);
            this.lastOffsetsMap = prepareRecords(client, streamIds, payload);
            client.close();
        }

        @SneakyThrows
        private byte[] randomPayload(int size) {
            byte[] payload = new byte[size];
            SecureRandom.getInstanceStrong().nextBytes(payload);
            return payload;
        }

        private Map<Long, Long> prepareRecords(OperationClient client, List<Long> streamIds, byte[] payload) {
            Map<Long, Long> lastOffsetsMap = new ConcurrentHashMap<>(streamIds.size());
            for (Long streamId : streamIds) {
                Long lastOffset = IntStream.range(0, PRE_APPEND_RECORD_COUNT)
                        .mapToObj(i -> append(client, streamId, payload))
                        .max(Long::compareTo)
                        .orElseThrow();
                lastOffsetsMap.put(streamId, lastOffset);
            }
            return lastOffsetsMap;
        }
    }

    @State(Scope.Group)
    @Getter
    public static class ClientState {
        private OperationClient client;

        @Setup
        public void setup(BenchmarkParams params, BenchmarkState benchmarkState) {
            int threadsPerClient = Arrays.stream(params.getThreadGroups()).sum();
            client = buildClient(benchmarkState.getPmAddress(), benchmarkState.getStreamCount() * 2, threadsPerClient * 2);
        }

        @TearDown
        @SneakyThrows
        public void tearDown() {
            client.close();
        }
    }

    @State(Scope.Thread)
    @Getter
    public static class ReadTailState {
        private Map<Long, Long> readOffsetsMap;
        private int streamIdIndex;

        @Setup
        public void setup(BenchmarkState benchmarkState) {
            this.readOffsetsMap = new ConcurrentHashMap<>(benchmarkState.getStreamIds().size());
            benchmarkState.getStreamIds().forEach(streamId -> readOffsetsMap.put(streamId, -1L));
        }

        @Setup(Level.Invocation)
        @SneakyThrows
        public void selectStreamId(BenchmarkState benchmarkState) {
            while (true) {
                streamIdIndex++;
                if (streamIdIndex >= benchmarkState.getStreamCount()) {
                    streamIdIndex = 0;
                }
                Long streamId = benchmarkState.getStreamIds().get(streamIdIndex);
                Long lastOffset = benchmarkState.getLastOffsetsMap().get(streamId);
                Long readOffset = readOffsetsMap.get(streamId);
                if (readOffset < lastOffset) {
                    break;
                }
            }
        }
    }

    @Benchmark
    @Group("readWrite")
    public void readRandom(BenchmarkState benchmarkState, ClientState clientState) {
        OperationClient client = clientState.getClient();
        Long streamId = benchmarkState.getStreamIds().get(random.nextInt(benchmarkState.getStreamCount()));
        Long lastOffset = benchmarkState.getLastOffsetsMap().get(streamId);
        long baseOffset = random.nextLong(0, lastOffset + 1);

        if (!fetchOne(client, streamId, baseOffset, benchmarkState.getBodySize())) {
            throw new RuntimeException("random read failed, streamId: " + streamId + ", baseOffset: " + baseOffset);
        }
    }

    @Benchmark
    @Group("readWrite")
    public void readTail(BenchmarkState benchmarkState, ClientState clientState, ReadTailState readTailState) {
        OperationClient client = clientState.getClient();
        Long streamId = benchmarkState.getStreamIds().get(readTailState.getStreamIdIndex());
        long baseOffset = readTailState.getReadOffsetsMap().get(streamId) + 1;

        Long readOffset = fetchFrom(client, streamId, baseOffset, benchmarkState.getBodySize());
        if (readOffset == null) {
            throw new RuntimeException("tail read failed, streamId: " + streamId + ", baseOffset: " + baseOffset);
        }
        readTailState.getReadOffsetsMap().put(streamId, readOffset);
    }

    @Benchmark
    @Group("readWrite")
    public long write(BenchmarkState benchmarkState, ClientState clientState) {
        OperationClient client = clientState.getClient();
        Long streamId = benchmarkState.getStreamIds().get(random.nextInt(benchmarkState.getStreamCount()));
        byte[] payload = benchmarkState.getPayload();

        long offset = append(client, streamId, payload);
        benchmarkState.getLastOffsetsMap().compute(streamId, (k, v) -> Optional.ofNullable(v).map(o -> Math.max(o, offset)).orElse(offset));
        return offset;
    }
}
