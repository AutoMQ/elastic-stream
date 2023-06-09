package com.automq.elasticstream.client.examples.benchmark;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Collections;

import org.openjdk.jmh.annotations.Benchmark;
import org.openjdk.jmh.annotations.BenchmarkMode;
import org.openjdk.jmh.annotations.OutputTimeUnit;
import org.openjdk.jmh.annotations.Scope;
import org.openjdk.jmh.annotations.Setup;
import org.openjdk.jmh.annotations.State;
import org.openjdk.jmh.runner.Runner;
import org.openjdk.jmh.runner.RunnerException;
import org.openjdk.jmh.runner.options.Options;
import org.openjdk.jmh.runner.options.OptionsBuilder;

import com.automq.elasticstream.client.DefaultRecordBatch;
import com.automq.elasticstream.client.FlatRecordBatchCodec;
import com.automq.elasticstream.client.api.Client;
import com.automq.elasticstream.client.api.CreateStreamOptions;
import com.automq.elasticstream.client.api.RecordBatch;
import com.automq.elasticstream.client.api.Stream;
import java.util.concurrent.TimeUnit;
import org.openjdk.jmh.annotations.Mode;

@BenchmarkMode(Mode.AverageTime)
@OutputTimeUnit(TimeUnit.NANOSECONDS)
public class AppendBench {
    @State(Scope.Benchmark)
    public static class Context {
        RecordBatch recordBatch = null;
        Stream stream = null;

        @Setup
        public void setup() throws Exception {
            byte[] payload = String.format("hello world %03d", 0).getBytes(StandardCharsets.UTF_8);
            ByteBuffer buffer = ByteBuffer.allocateDirect(payload.length);
            recordBatch = new DefaultRecordBatch(1, 0, Collections.emptyMap(), buffer);

            Client client = Client.builder().endpoint("127.0.0.1:12378").kvEndpoint("127.0.0.1:12379").build();
            stream = client.streamClient()
                    .createAndOpenStream(CreateStreamOptions.newBuilder().replicaCount(1).build()).get();
        }
    }

    @Benchmark
    public void testEncode(Context ctx) {
        ctx.stream.append(ctx.recordBatch);
    }

    public static void main(String... args) throws RunnerException {
        Options opt = new OptionsBuilder()
                .include(AppendBench.class.getSimpleName())
                .forks(1)
                .build();
        new Runner(opt).run();
    }
}
