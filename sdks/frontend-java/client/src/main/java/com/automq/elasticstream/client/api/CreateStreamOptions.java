package com.automq.elasticstream.client.api;

import com.automq.elasticstream.client.utils.Arguments;

public class CreateStreamOptions {
    private int replicaCount;
    private long epoch;

    public static Builder newBuilder() {
        return new Builder();
    }

    public int replicaCount() {
        return replicaCount;
    }

    public long epoch() {
        return epoch;
    }

    public static class Builder {
        private final CreateStreamOptions options = new CreateStreamOptions();

        public Builder replicaCount(int replicaCount) {
            Arguments.check(replicaCount > 0, "replica count should larger than 0");
            options.replicaCount = replicaCount;
            return this;
        }

        public Builder epoch(long epoch) {
            options.epoch = epoch;
            return this;
        }

        public CreateStreamOptions build() {
            return options;
        }

    }
}
