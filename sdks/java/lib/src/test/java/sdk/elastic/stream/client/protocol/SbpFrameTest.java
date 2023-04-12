package sdk.elastic.stream.client.protocol;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.nio.ByteBuffer;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

class SbpFrameTest {

    @Test
    void testEncodeAndDecode() {
        SbpFrameBuilder defaultBuilder = SbpFrame.newBuilder()
            .setMagicCode((byte) 0x01)
            .setOperationCode((short) 0x02)
            .setFlag((byte) 0x03)
            .setStreamId(0x04);
        defaultBuilder.setHeader(ByteBuffer.wrap(new byte[] {0x05, 0x06, 0x07}));
        defaultBuilder.setPayload(new ByteBuffer[] {ByteBuffer.wrap(new byte[] {0x08, 0x09, 0x0a})});
        SbpFrame frame = defaultBuilder.build();

        ByteBuf encoded = Unpooled.wrappedBuffer(frame.encode());
        SbpFrame decoded = new SbpFrame(encoded.duplicate());
        assertEquals(frame, decoded);

        defaultBuilder.setPayload(null);
        defaultBuilder.build();
        defaultBuilder.setHeader(null);
        defaultBuilder.build();

        assertThrows(NullPointerException.class, () -> new SbpFrame(null));
        assertThrows(AssertionError.class, () -> new SbpFrame(Unpooled.wrappedBuffer(new byte[] {0x01, 0x02, 0x03})));

        // invalid header length
        defaultBuilder.setHeader(ByteBuffer.wrap(new byte[SbpFrame.MAX_HEADER_LENGTH + 1]));
        assertThrows(IllegalArgumentException.class, defaultBuilder::build);
        defaultBuilder.setHeader(null);

        // inconsistent total length
        int originalLength = encoded.readableBytes();
        int tooBigLength = originalLength + 1;
        changeFirst4BytesWithIntValue(encoded, tooBigLength);
        assertThrows(IllegalArgumentException.class, () -> new SbpFrame(encoded.duplicate()));
        changeFirst4BytesWithIntValue(encoded, originalLength);

        // invalid crc
        encoded.setByte(encoded.readableBytes() - 1, (byte) ~encoded.getByte(encoded.readableBytes() - 1));
        assertThrows(IllegalArgumentException.class, () -> new SbpFrame(encoded.duplicate()));
    }

    private void changeFirst4BytesWithIntValue(ByteBuf buffer, int newValue) {
        assert buffer != null && buffer.readableBytes() >= 4;
        buffer.setInt(0, newValue);
    }
}