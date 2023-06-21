package com.automq.elasticstream.client.utils;

import java.lang.reflect.Field;
import java.nio.Buffer;
import java.nio.ByteBuffer;

public class BytesUtils {
    static Field BUFFER_ADDRESS_FIELD;
    static {
        try {
            BUFFER_ADDRESS_FIELD = Buffer.class.getDeclaredField("address");
            BUFFER_ADDRESS_FIELD.setAccessible(true);
        } catch (NoSuchFieldException e) {
            throw new RuntimeException(e);
        }
    }

    public static long getAddress(ByteBuffer buffer) {
        if (!buffer.isDirect()) {
            throw new IllegalArgumentException("buffer is not direct buffer");
        }
        try {
            return BUFFER_ADDRESS_FIELD.getLong(buffer);
        } catch (IllegalAccessException e) {
            throw new RuntimeException(e);
        }
    }
    
}
