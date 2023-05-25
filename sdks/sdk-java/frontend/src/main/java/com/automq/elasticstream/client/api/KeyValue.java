package com.automq.elasticstream.client.api;


import java.nio.ByteBuffer;

public class KeyValue {
    private final String key;
    private final ByteBuffer value;

    private KeyValue(String key, ByteBuffer value) {
        this.key = key;
        this.value = value;
    }

    public static KeyValue of(String key, ByteBuffer value) {
        return new KeyValue(key, value);
    }

    public String key() {
        return key;
    }

    public ByteBuffer value() {
        return value;
    }

    @Override
    public boolean equals(Object o) {
        if (o == this) {
            return true;
        }
        if (!(o instanceof KeyValue)) {
            return false;
        }
        final KeyValue other = (KeyValue) o;
        return new org.apache.commons.lang3.builder.EqualsBuilder()
                .append(key, other.key)
                .append(value.slice(), other.value.slice())
                .isEquals();
    }
}
