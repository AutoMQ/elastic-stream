package com.automq.elasticstream.client.examples.benchmark;

import com.automq.elasticstream.client.DefaultRecordBatch;
import com.automq.elasticstream.client.api.Client;
import com.automq.elasticstream.client.api.CreateStreamOptions;
import com.automq.elasticstream.client.api.Stream;
import com.automq.elasticstream.client.api.StreamClient;

import java.nio.ByteBuffer;
import java.util.Collections;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.locks.LockSupport;

/**
 * How to run:
 * 1. cd sdks/frontend-java; mvn clean install
 * 2. SDK_BENCHMARK_ENDPOINT=127.0.0.1 SDK_BENCHMARK_CONFIG=1,1048576,1,1,1,1 java -cp examples/target/examples-1.0-SNAPSHOT-jar-with-dependencies.jar com.automq.elasticstream.client.examples.benchmark.StreamBench
 *     SDK_BENCHMARK_CONFIG format =  throughput(MB),recordSize(byte),duration(minute),threadCount,streamCountPerThread,replicaCount
 */
class StreamBench {
    private final StreamClient streamClient;

    public StreamBench(String endpoint, int throughput, int recordSize, long duration, int threadCount, int streamCount, int replicaCount) {
        Client client = Client.builder().endpoint(endpoint + ":12378").kvEndpoint(endpoint + ":12379").build();
        streamClient = client.streamClient();

        for (int i = 0; i < threadCount; i++) {
            int taskIndex = i;
            new Thread(() -> {
                try {
                    runTask(taskIndex, throughput * 1024 * 1024 / threadCount, recordSize, duration, streamCount, replicaCount);
                } catch (Exception e) {
                    e.printStackTrace();
                }
            }).start();
        }
    }

    public static void main(String... args) throws Exception {
        String endpoint = System.getenv("SDK_BENCHMARK_ENDPOINT");
        String config = System.getenv("SDK_BENCHMARK_CONFIG");
        if (config == null || config.isEmpty()) {
            throw new IllegalArgumentException("invalid config: " + config + ", should be throughput(MB),recordSize(byte),duration(minute),threadCount,streamCountPerThread,replicaCount");
        }
        String[] parts = config.split(",");
        if (parts.length != 6) {
            throw new IllegalArgumentException("invalid config: " + config + ", should be throughput(MB),recordSize(byte),duration(minute),threadCount,streamCountPerThread,replicaCount");
        }
        int throughput = Integer.parseInt(parts[0]); // MB
        int recordSize = Integer.parseInt(parts[1]); // byte
        long duration = Long.parseLong(parts[2]) * 60 * 1000; // minute to mills
        int threadCount = Integer.parseInt(parts[3]);
        int streamCount = Integer.parseInt(parts[4]);
        int replicaCount = Integer.parseInt(parts[5]);
        new StreamBench(endpoint, throughput, recordSize, duration, threadCount, streamCount, replicaCount);
    }

    private void runTask(int taskIndex, int throughput, int recordSize, long duration, int streamCount, int replicaCount) throws Exception {
        System.out.println(" task-" + taskIndex + " start");
        int intervalNanos = 1000 * 1000 * 1000 / (Math.max(throughput / recordSize, 1));
        long lastTimestamp = System.nanoTime();
        Stream[] streams = new Stream[streamCount];
        for (int i = 0; i < streamCount; i++) {
            streams[i] = streamClient.createAndOpenStream(CreateStreamOptions.newBuilder().replicaCount(replicaCount).build()).get();
        }
        int streamIndex = 0;
        ByteBuffer payload = ByteBuffer.wrap(new byte[recordSize]);

        long lastLogTimestamp = System.currentTimeMillis();
        long taskStartTimestamp = System.currentTimeMillis();
        AtomicLong count = new AtomicLong();
        AtomicLong costNanos = new AtomicLong();
        while (true) {
            while (true) {
                long now = System.nanoTime();
                long elapsed = now - lastTimestamp;
                if (elapsed < intervalNanos) {
                    LockSupport.parkNanos(elapsed);
                } else {
                    lastTimestamp += intervalNanos;
                    break;
                }
            }
            long now = System.currentTimeMillis();
            if (now - taskStartTimestamp > duration) {
                System.out.println(now + " task-" + taskIndex + " finished");
                break;
            }
            if (now - lastLogTimestamp > 1000) {
                long countValue = count.getAndSet(0);
                long costNanosValue = costNanos.getAndSet(0);
                if (countValue != 0) {
                    System.out.println(now + " task-" + taskIndex + " avg=" + costNanosValue / countValue / 1000 + "us, count=" + countValue);
                }
                lastLogTimestamp = now;
            }


            Stream stream = streams[Math.abs(streamIndex++ % streamCount)];
            long appendStartNano = System.nanoTime();
            stream.append(new DefaultRecordBatch(10, 233, Collections.emptyMap(), payload.duplicate()))
                    .thenAccept(rst -> {
                        count.incrementAndGet();
                        costNanos.addAndGet(System.nanoTime() - appendStartNano);
                    });
        }

    }
}
