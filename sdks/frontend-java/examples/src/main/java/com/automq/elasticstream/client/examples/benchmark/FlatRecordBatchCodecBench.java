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
import com.automq.elasticstream.client.api.RecordBatch;
import java.util.concurrent.TimeUnit;
import org.openjdk.jmh.annotations.Mode;

@BenchmarkMode(Mode.AverageTime)
@OutputTimeUnit(TimeUnit.NANOSECONDS)
public class FlatRecordBatchCodecBench {
    @State(Scope.Benchmark)
    public static class Context {
        RecordBatch recordBatch = null;

        @Setup
        public void setup() {
            byte[] payload = String.format("hello world %03d", 0).getBytes(StandardCharsets.UTF_8);
            ByteBuffer buffer = ByteBuffer.allocateDirect(payload.length);
            recordBatch = new DefaultRecordBatch(1, 0, Collections.emptyMap(), buffer);
        }
    }

    @Benchmark
    public void testEncode(Context ctx) {
        FlatRecordBatchCodec.encode(233, ctx.recordBatch).release();
    }

    public static void main(String... args) throws RunnerException {
        Options opt = new OptionsBuilder()
                .include(FlatRecordBatchCodec.class.getSimpleName())
                .forks(1)
                .build();
        new Runner(opt).run();
    }
}
