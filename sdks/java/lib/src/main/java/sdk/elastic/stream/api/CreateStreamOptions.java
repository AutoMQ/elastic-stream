package sdk.elastic.stream.api;

import sdk.elastic.stream.utils.Arguments;

public class CreateStreamOptions {
    private int replicaCount;

    public static Builder newBuilder() {
        return new Builder();
    }

    public int replicaCount() {
        return replicaCount;
    }

    public static class Builder {
        private final CreateStreamOptions options = new CreateStreamOptions();

        public Builder replicaCount(int replicaCount) {
            Arguments.check(replicaCount > 0, "replica count should larger than 0");
            options.replicaCount = replicaCount;
            return this;
        }

        public CreateStreamOptions build() {
            return options;
        }

    }
}
