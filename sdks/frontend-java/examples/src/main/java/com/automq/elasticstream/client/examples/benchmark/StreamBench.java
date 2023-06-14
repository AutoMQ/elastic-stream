package com.automq.elasticstream.client.examples.benchmark;

import com.automq.elasticstream.client.DefaultRecordBatch;
import com.automq.elasticstream.client.api.Client;
import com.automq.elasticstream.client.api.CreateStreamOptions;
import com.automq.elasticstream.client.api.FetchResult;
import com.automq.elasticstream.client.api.RecordBatchWithContext;
import com.automq.elasticstream.client.api.Stream;
import com.automq.elasticstream.client.api.StreamClient;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.locks.LockSupport;
import java.util.function.Consumer;

/**
 * How to run:
 * 1. cd sdks/; ./build.sh; cd frontend-java
 * 2.
 * SDK_BENCHMARK_ENDPOINT=127.0.0.1 \
 * SDK_BENCHMARK_CONFIG=throughput=1,recordSize=1048576,duration=1,taskCount=1,streamPerThread=1,replicaCount=1 \
 * SDK_FETCH_CONFIG=fetchSize=65536,taskCount=1 \
 * java -cp examples/target/examples-1.0-SNAPSHOT-jar-with-dependencies.jar com.automq.elasticstream.client.examples.benchmark.StreamBench
 */
class StreamBench {
    private final StreamClient streamClient;

    public StreamBench(String endpoint, AppendTaskConfig appendTaskConfig, FetchTaskConfig fetchTaskConfig) {
        Client client = Client.builder().endpoint(endpoint + ":12378").kvEndpoint(endpoint + ":12379").build();
        streamClient = client.streamClient();
        for (int i = 0; i < appendTaskConfig.taskCount; i++) {
            int taskIndex = i;
            new Thread(() -> {
                try {
                    runTask(taskIndex, appendTaskConfig, fetchTaskConfig);
                } catch (Exception e) {
                    e.printStackTrace();
                }
            }).start();
        }
    }

    public static void main(String... args) throws Exception {
        String endpoint = System.getenv("SDK_BENCHMARK_ENDPOINT");

        AppendTaskConfig appendTaskConfig = AppendTaskConfig.parse(System.getenv("SDK_BENCHMARK_CONFIG"));
        FetchTaskConfig fetchTaskConfig = FetchTaskConfig.parse(System.getenv("SDK_FETCH_CONFIG"));
        if (fetchTaskConfig.isFetchOn()) {
            if (fetchTaskConfig.taskCount < appendTaskConfig.taskCount || fetchTaskConfig.taskCount % appendTaskConfig.taskCount != 0) {
                throw new IllegalArgumentException("invalid config: SDK_FETCH_CONFIG.taskCount should be multiple of SDK_BENCHMARK_CONFIG.taskCount");
            }
            if (fetchTaskConfig.taskCount > appendTaskConfig.streamCount || appendTaskConfig.streamCount % fetchTaskConfig.taskCount != 0) {
                throw new IllegalArgumentException("invalid config: SDK_BENCHMARK_CONFIG.streamPerThread should be multiple of SDK_FETCH_CONFIG.taskCount");
            }
        }
        new StreamBench(endpoint, appendTaskConfig, fetchTaskConfig);
    }

    private void runTask(int taskIndex, AppendTaskConfig appendTaskConfig, FetchTaskConfig fetchTaskConfig) throws Exception {
        System.out.println("task-" + taskIndex + " start");
        List<Stream> streams = new ArrayList<>(appendTaskConfig.streamCount);
        AtomicBoolean doneSignal = new AtomicBoolean();
        for (int i = 0; i < appendTaskConfig.streamCount; i++) {
            streams.add(streamClient.createAndOpenStream(CreateStreamOptions.newBuilder().replicaCount(appendTaskConfig.replicaCount).build()).get());
        }
        Consumer<ConfirmEvent> confirmEventListener;
        if (fetchTaskConfig.isFetchOn()) {
            int fetchForTaskCount = fetchTaskConfig.taskCount / appendTaskConfig.taskCount;
            List<BlockingQueue<ConfirmEvent>> streamConfirmQueues = new ArrayList<>(fetchForTaskCount);
            for (int i = 0; i < fetchForTaskCount; i++) {
                streamConfirmQueues.add(new LinkedBlockingQueue<>());
            }

            int fetchStreamSplitFactor = streams.size() / fetchForTaskCount;

            for (int i = 0; i < fetchForTaskCount; i++) {
                String taskId = taskIndex + "#" + i;
                List<Stream> fetchStreams = streams.subList(i * fetchStreamSplitFactor, (i + 1) * fetchStreamSplitFactor);
                BlockingQueue<ConfirmEvent> confirmEventQueue = streamConfirmQueues.get(i);
                new Thread(() -> {
                    try {
                        runFetchTask(taskId, fetchStreams, confirmEventQueue, doneSignal, fetchTaskConfig);
                    } catch (Throwable e) {
                        e.printStackTrace();
                    }
                }).start();
            }
            confirmEventListener = event -> {
                //noinspection ResultOfMethodCallIgnored
                streamConfirmQueues.get(event.streamIndex / fetchStreamSplitFactor).offer(event);
            };
        } else {
            confirmEventListener = event -> {};
        }

        runAppendTask(taskIndex, streams, confirmEventListener, doneSignal, appendTaskConfig);
    }

    private void runFetchTask(String taskId, List<Stream> streams, BlockingQueue<ConfirmEvent> confirmEventQueue, AtomicBoolean doneSignal, FetchTaskConfig fetchTaskConfig) throws InterruptedException, ExecutionException {
        int streamIndex = 0;
        Map<Stream, AtomicLong> confirmOffsets = new HashMap<>();
        Map<Stream, AtomicLong> nextPullOffsets = new HashMap<>();
        streams.forEach(stream -> {
            confirmOffsets.put(stream, new AtomicLong());
            nextPullOffsets.put(stream, new AtomicLong());
        });

        long lastLogTimestamp = System.currentTimeMillis();

        int emptyRound = 0;
        AtomicLong count = new AtomicLong();
        AtomicLong costNanos = new AtomicLong();
        AtomicLong throughput = new AtomicLong();
        while (!doneSignal.get()) {
            while (true) {
                ConfirmEvent event = confirmEventQueue.poll();
                if (event == null) {
                    break;
                }
                AtomicLong confirmOffset = confirmOffsets.get(event.stream);
                if (confirmOffset.get() < event.confirmOffset) {
                    confirmOffset.set(event.confirmOffset);
                }
            }
            Stream stream = streams.get(Math.abs(streamIndex++ % streams.size()));
            AtomicLong nextPullOffsetRef = nextPullOffsets.get(stream);
            long nextPullOffset = nextPullOffsetRef.get();
            long confirmOffset = confirmOffsets.get(stream).get();
            if (confirmOffset == nextPullOffset) {
                if (++emptyRound > streams.size()) {
                    //noinspection BusyWait
                    Thread.sleep(10);
                    emptyRound = 0;
                }
                continue;
            }
            long startNanos = System.nanoTime();
            FetchResult fetchResult = stream.fetch(nextPullOffset, confirmOffset, fetchTaskConfig.fetchSize).get();
            emptyRound = 0;
            long costNanosValue = costNanos.addAndGet(System.nanoTime() - startNanos);
            long countValue = count.incrementAndGet();
            long fetchResultSize = fetchResult.recordBatchList().stream().mapToLong(r -> r.rawPayload().remaining()).sum();
            long throughputValue = throughput.addAndGet(fetchResultSize);

            RecordBatchWithContext last = fetchResult.recordBatchList().get(fetchResult.recordBatchList().size() - 1);
            nextPullOffsetRef.set(last.lastOffset());
            fetchResult.free();

            long now = System.currentTimeMillis();
            if (now - lastLogTimestamp > 1000) {
                if (countValue != 0) {
                    System.out.println(now + " fetch task-" + taskId + " avg=" + costNanosValue / countValue / 1000 + "us, fetchRequestCount=" + countValue + " throughput=" + (throughputValue / 1024) + "KB/s");
                    costNanos.set(0);
                    count.set(0);
                    throughput.set(0);
                }
                lastLogTimestamp = now;
            }
        }
    }

    private void runAppendTask(int taskIndex, List<Stream> streams, Consumer<ConfirmEvent> confirmEventListener, AtomicBoolean doneSignal, AppendTaskConfig appendTaskConfig) {
        System.out.println("append task-" + taskIndex + " start");
        int streamIndex = 0;
        ByteBuffer payload = ByteBuffer.wrap(new byte[appendTaskConfig.recordSize]);
        int intervalNanos = 1000 * 1000 * 1000 / (Math.max(appendTaskConfig.throughput / appendTaskConfig.recordSize, 1));
        long lastTimestamp = System.nanoTime();
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
            if (now - taskStartTimestamp > appendTaskConfig.duration) {
                System.out.println(now + " append task-" + taskIndex + " finished");
                break;
            }
            if (now - lastLogTimestamp > 1000) {
                long countValue = count.getAndSet(0);
                long costNanosValue = costNanos.getAndSet(0);
                if (countValue != 0) {
                    System.out.println(now + " append task-" + taskIndex + " avg=" + costNanosValue / countValue / 1000 + "us, count=" + countValue + " throughput=" + (countValue * appendTaskConfig.recordSize / 1024) + "KB/s");
                }
                lastLogTimestamp = now;
            }


            int currentStreamIndex = Math.abs(streamIndex++ % appendTaskConfig.streamCount);
            Stream stream = streams.get(currentStreamIndex);
            long appendStartNano = System.nanoTime();
            stream.append(new DefaultRecordBatch(10, 233, Collections.emptyMap(), payload.duplicate()))
                    .thenAccept(rst -> {
                        count.incrementAndGet();
                        costNanos.addAndGet(System.nanoTime() - appendStartNano);
                        confirmEventListener.accept(new ConfirmEvent(stream, currentStreamIndex, rst.baseOffset() + 10));
                    });
        }
        doneSignal.set(true);
    }

    static class ConfirmEvent {
        public final Stream stream;
        public final int streamIndex;
        public final long confirmOffset;

        public ConfirmEvent(Stream stream, int streamIndex, long confirmOffset) {
            this.stream = stream;
            this.streamIndex = streamIndex;
            this.confirmOffset = confirmOffset;
        }
    }

    static class AppendTaskConfig {
        public final int throughput; // byte
        public final int recordSize; // byte
        public final long duration; // mills
        public final int streamCount;
        public final int replicaCount;
        public final int taskCount;

        public AppendTaskConfig(int throughput, int recordSize, long duration, int streamCount, int replicaCount, int appendTaskCount) {
            this.throughput = throughput;
            this.recordSize = recordSize;
            this.duration = duration;
            this.streamCount = streamCount;
            this.replicaCount = replicaCount;
            this.taskCount = appendTaskCount;
        }

        public static AppendTaskConfig parse(String str) {
            if (str == null || str.isEmpty()) {
                throw new IllegalArgumentException("invalid config: " + str + ", should be throughput(MB),recordSize(byte),duration(minute),threadCount,streamCountPerThread,replicaCount");
            }
            String[] parts = str.split(",");
            if (parts.length != 6) {
                throw new IllegalArgumentException("invalid config: " + str + ", should be throughput(MB),recordSize(byte),duration(minute),threadCount,streamCountPerThread,replicaCount");
            }
            int throughput = Integer.parseInt(parts[0].split("=")[1]) * 1024 * 1024; // MB to byte
            int recordSize = Integer.parseInt(parts[1].split("=")[1]); // byte
            long duration = Long.parseLong(parts[2].split("=")[1]) * 60 * 1000; // minute to mills
            int appendTaskCount = Integer.parseInt(parts[3].split("=")[1]);
            int streamCount = Integer.parseInt(parts[4].split("=")[1]);
            int replicaCount = Integer.parseInt(parts[5].split("=")[1]);
            return new AppendTaskConfig(throughput / appendTaskCount, recordSize, duration, streamCount, replicaCount, appendTaskCount);
        }
    }

    static class FetchTaskConfig {
        private final int fetchSize; // byte
        private final int taskCount;

        public FetchTaskConfig(int fetchSize, int taskCount) {
            this.fetchSize = fetchSize;
            this.taskCount = taskCount;
        }

        public boolean isFetchOn() {
            return taskCount != 0;
        }

        public static FetchTaskConfig parse(String str) {
            if (str == null || str.isEmpty()) {
                throw new IllegalArgumentException("invalid config: " + str + ", should be fetchSize(byte),fetchTaskCount(the fetchTaskCount must be a multiple of appendTaskCount)");
            }
            String[] parts = str.split(",");
            if (parts.length != 2) {
                throw new IllegalArgumentException("invalid config: " + str + ", should be fetchSize(byte),fetchTaskCount(the fetchTaskCount must be a multiple of appendTaskCount)");
            }
            int fetchSize = Integer.parseInt(parts[0].split("=")[1]);
            int taskCount = Integer.parseInt(parts[1].split("=")[1]);
            return new FetchTaskConfig(fetchSize, taskCount);
        }
    }
}
