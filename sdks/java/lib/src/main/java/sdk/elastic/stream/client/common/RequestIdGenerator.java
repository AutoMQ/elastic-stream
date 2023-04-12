package sdk.elastic.stream.client.common;

import java.util.concurrent.atomic.AtomicInteger;

public class RequestIdGenerator {
    private final AtomicInteger index;

    public RequestIdGenerator() {
        this.index = new AtomicInteger(0);
    }

    /**
     * Get a new id for this instance.
     * Since we only need a unique id, there is no need to handle the situation when index comes into Integer.MAX_VALUE.
     *
     * @return index.
     */
    public int getId() {
        return index.getAndIncrement();
    }

    @Override
    public String toString() {
        return String.valueOf(index.get());
    }
}
