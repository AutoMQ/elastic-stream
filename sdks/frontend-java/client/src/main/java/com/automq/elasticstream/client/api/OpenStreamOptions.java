package com.automq.elasticstream.client.api;

import com.automq.elasticstream.client.utils.Arguments;

public class OpenStreamOptions {
    private WriteMode writeMode = WriteMode.SINGLE;
    private ReadMode readMode = ReadMode.MULTIPLE;
    private long epoch;

    public static Builder newBuilder() {
        return new Builder();
    }

    public WriteMode writeMode() {
        return writeMode;
    }

    public ReadMode readMode() {
        return readMode;
    }

    public long epoch() {
        return epoch;
    }

    public enum WriteMode {
        SINGLE(0), MULTIPLE(1);

        final int code;

        WriteMode(int code) {
            this.code = code;
        }

        public int getCode() {
            return code;
        }
    }

    public enum ReadMode {
        SINGLE(0), MULTIPLE(1);

        final int code;

        ReadMode(int code) {
            this.code = code;
        }

        public int getCode() {
            return code;
        }
    }

    public static class Builder {
        private final OpenStreamOptions options = new OpenStreamOptions();

        public Builder writeMode(WriteMode writeMode) {
            Arguments.isNotNull(writeMode, "WriteMode should be set with SINGLE or MULTIPLE");
            options.writeMode = writeMode;
            return this;
        }

        public Builder readMode(ReadMode readMode) {
            Arguments.isNotNull(readMode, "ReadMode should be set with SINGLE or MULTIPLE");
            options.readMode = readMode;
            return this;
        }

        public Builder epoch(long epoch) {
            options.epoch = epoch;
            return this;
        }

        public OpenStreamOptions build() {
            return options;
        }
    }
}
