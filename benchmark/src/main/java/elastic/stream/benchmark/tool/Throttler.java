package elastic.stream.benchmark.tool;

import java.util.concurrent.TimeUnit;

/**
 * Thread safe throttler for rate limiting.
 *
 * @author ningyu
 */
public class Throttler {

    private static final long MIN_SLEEP_NS = TimeUnit.MILLISECONDS.toNanos(2);

    private final long sleepPerTicketNs;
    private final long ticketPerSecond;
    private final Object mutex = new Object();

    private long startNs;

    private long tickets = 0;
    private long toSleepNs = 0;
    private boolean stopped = false;

    /**
     * @param ticketPerSecond ticket per second.
     */
    public Throttler(long ticketPerSecond) {
        this.ticketPerSecond = ticketPerSecond;
        this.sleepPerTicketNs = TimeUnit.SECONDS.toNanos(1) / ticketPerSecond;
    }

    /**
     * Start to throttle.
     */
    public void start() {
        this.startNs = System.nanoTime();
    }

    /**
     * Stop to throttle, wake up all threads.
     */
    public void stop() {
        stopped = true;
        synchronized (mutex) {
            mutex.notifyAll();
        }
    }

    /**
     * Optionally sleep to throttle.
     */
    public synchronized void throttle() {
        tickets += 1;

        float elapsedSec = 1.0f * (System.nanoTime() - startNs) / TimeUnit.SECONDS.toNanos(1);
        if (tickets / elapsedSec <= ticketPerSecond) {
            return;
        }

        toSleepNs += sleepPerTicketNs;
        if (toSleepNs >= MIN_SLEEP_NS) {
            long sleepStartNs = System.nanoTime();
            try {
                long remainingNs = toSleepNs;
                synchronized (mutex) {
                    while (!stopped && remainingNs > 0) {
                        TimeUnit.NANOSECONDS.timedWait(mutex, remainingNs);
                        remainingNs = toSleepNs - (System.nanoTime() - sleepStartNs);
                    }
                }
                toSleepNs = 0;
            } catch (InterruptedException e) {
                long elapsedNs = System.nanoTime() - sleepStartNs;
                if (elapsedNs <= toSleepNs) {
                    toSleepNs -= elapsedNs;
                } else {
                    toSleepNs = 0;
                }
            }
        }
    }
}
