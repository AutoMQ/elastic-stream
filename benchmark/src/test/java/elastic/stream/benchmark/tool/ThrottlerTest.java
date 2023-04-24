package elastic.stream.benchmark.tool;

import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;

class ThrottlerTest {

    @Test
    void testThrottle() {
        final long ticketPerSecond = 16384;
        Throttler throttler = new Throttler(ticketPerSecond);

        long start = System.nanoTime();
        throttler.start();
        for (int i = 0; i < ticketPerSecond; i++) {
            throttler.throttle();
        }
        long end = System.nanoTime();
        throttler.stop();

        // assert that the time elapsed is about 1 second
        Assertions.assertTrue((end - start) > TimeUnit.SECONDS.toNanos(1) * 0.997);
        Assertions.assertTrue((end - start) < TimeUnit.SECONDS.toNanos(1) * 1.003);
    }

    @Test
    void testThrottleThreadSafe() {
        final int ticketPerSecond = 16384;

        Throttler throttler = new Throttler(ticketPerSecond);
        ExecutorService executor = Executors.newFixedThreadPool(16);
        CompletableFuture[] futures = new CompletableFuture[ticketPerSecond];

        long start = System.nanoTime();
        throttler.start();

        for (int i = 0; i < ticketPerSecond; i++) {
            futures[i] = CompletableFuture.runAsync(throttler::throttle, executor);
        }
        CompletableFuture.allOf(futures).join();

        long end = System.nanoTime();
        throttler.stop();

        // assert that the time elapsed is about 1 second
        Assertions.assertTrue((end - start) > TimeUnit.SECONDS.toNanos(1) * 0.994);
        Assertions.assertTrue((end - start) < TimeUnit.SECONDS.toNanos(1) * 1.006);
    }

    @Test
    void testStop() {
        Throttler throttler = new Throttler(1);
        ExecutorService executor = Executors.newFixedThreadPool(16);
        CompletableFuture[] futures = new CompletableFuture[100];

        // new thread to stop the throttler
        new Thread(() -> {
            try {
                Thread.sleep(100);
            } catch (InterruptedException e) {
                Assertions.fail();
            }
            throttler.stop();
        }).start();

        long start = System.nanoTime();
        throttler.start();

        for (int i = 0; i < 100; i++) {
            futures[i] = CompletableFuture.runAsync(throttler::throttle, executor);
        }
        CompletableFuture.allOf(futures).join();

        long end = System.nanoTime();

        Assertions.assertTrue((end - start) < TimeUnit.SECONDS.toNanos(1));
        Assertions.assertTrue((end - start) >= TimeUnit.MILLISECONDS.toNanos(100));
    }
}
