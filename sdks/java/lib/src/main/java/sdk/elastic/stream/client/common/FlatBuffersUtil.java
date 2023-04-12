package sdk.elastic.stream.client.common;

import com.google.flatbuffers.ByteVector;
import java.nio.ByteBuffer;

public class FlatBuffersUtil {
    /**
     * Convert a ByteVector to a byte buffer.
     *
     * @param byteVector the byte vector
     * @return the byte buffer
     */
    public static ByteBuffer byteVector2ByteBuffer(ByteVector byteVector) {
        assert byteVector != null && byteVector.length() > 0;
        ByteBuffer byteBuffer = ByteBuffer.allocate(byteVector.length());
        for (int i = 0; i < byteVector.length(); i++) {
            byteBuffer.put(byteVector.get(i));
        }
        return byteBuffer;
    }
}
