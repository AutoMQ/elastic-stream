package elastic.stream.benchmark.jmh;

import lombok.Getter;
import lombok.SneakyThrows;
import org.openjdk.jmh.annotations.Benchmark;
import org.openjdk.jmh.annotations.Group;
import org.openjdk.jmh.annotations.Param;
import org.openjdk.jmh.annotations.Scope;
import org.openjdk.jmh.annotations.Setup;
import org.openjdk.jmh.annotations.State;
import org.openjdk.jmh.annotations.TearDown;
import sdk.elastic.stream.apis.OperationClient;

import java.security.SecureRandom;
import java.util.List;
import java.util.Random;
import java.util.stream.IntStream;

import static elastic.stream.benchmark.tool.ClientTool.append;
import static elastic.stream.benchmark.tool.ClientTool.buildClient;
import static elastic.stream.benchmark.tool.ClientTool.createStreams;
import static elastic.stream.benchmark.tool.ClientTool.fetch;

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
        @Param({"1024"})
        private int bodySize;
        @Param({"64"})
        private int streamCount;

        private byte[] payload;
        private List<Long> streamIds;
        private List<Long> baseOffsets;

        @Setup
        public void setup() throws Exception {
            OperationClient client = buildClient(pmAddress, streamCount * 2);
            this.streamIds = createStreams(client, streamCount);
            this.payload = randomPayload(bodySize);
            this.baseOffsets = prepareRecords(client, streamIds.get(0), payload);
            client.close();
        }

        @SneakyThrows
        private byte[] randomPayload(int size) {
            byte[] payload = new byte[size];
            SecureRandom.getInstanceStrong().nextBytes(payload);
            return payload;
        }

        private List<Long> prepareRecords(OperationClient client, long streamId, byte[] payload) {
            return IntStream.range(0, PRE_APPEND_RECORD_COUNT).mapToObj(i -> append(client, streamId, payload)).toList();
        }
    }

    @State(Scope.Group)
    @Getter
    public static class ClientState {
        private OperationClient client;

        @Setup
        public void setup(BenchmarkState benchmarkState) {
            client = buildClient(benchmarkState.getPmAddress(), benchmarkState.getStreamCount() * 2);
        }

        @TearDown
        @SneakyThrows
        public void tearDown() {
            client.close();
        }
    }

    @Benchmark
    @Group("readWrite")
    public boolean read(BenchmarkState benchmarkState, ClientState clientState) {
        OperationClient client = clientState.getClient();
        // TODO random read
        Long streamId = benchmarkState.getStreamIds().get(0);
        Long baseOffset = benchmarkState.getBaseOffsets().get(random.nextInt(PRE_APPEND_RECORD_COUNT));

        return fetch(client, streamId, baseOffset, benchmarkState.getBodySize());
    }

    @Benchmark
    @Group("readWrite")
    public long write(BenchmarkState benchmarkState, ClientState clientState) {
        OperationClient client = clientState.getClient();
        Long streamId = benchmarkState.getStreamIds().get(random.nextInt(benchmarkState.getStreamCount()));
        byte[] payload = benchmarkState.getPayload();

        return append(client, streamId, payload);
    }
}
