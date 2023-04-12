package sdk.elastic.stream.client.netty;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import io.netty.channel.embedded.EmbeddedChannel;
import java.nio.ByteBuffer;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import sdk.elastic.stream.client.protocol.SbpFrame;
import sdk.elastic.stream.client.protocol.SbpFrameBuilder;

class NettyDecoderTest {
    private static SbpFrameBuilder defaultBuilder;

    @BeforeAll
    static void setUp() {
        defaultBuilder = SbpFrame.newBuilder()
            .setMagicCode((byte) 0x01)
            .setOperationCode((short) 0x02)
            .setFlag((byte) 0x03)
            .setStreamId(0x04);
        defaultBuilder.setHeader(ByteBuffer.wrap(new byte[] {0x05, 0x06, 0x07}));
        defaultBuilder.setPayload(new ByteBuffer[] {ByteBuffer.wrap(new byte[] {0x08, 0x09, 0x0a})});
    }

    @Test
    void testDecode() {
        SbpFrame frame = defaultBuilder.build();
        ByteBuf buf = Unpooled.wrappedBuffer(frame.encode());

        EmbeddedChannel channel = new EmbeddedChannel(new NettyDecoder());
        Assertions.assertTrue(channel.writeInbound(buf));
        Assertions.assertTrue(channel.finish());

        SbpFrame outFrame = channel.readInbound();
        Assertions.assertEquals(frame, outFrame);
        Assertions.assertNull(channel.readInbound());
    }

    @Test
    void testDecode2() {
        SbpFrame frame = defaultBuilder.build();
        ByteBuf buf = Unpooled.wrappedBuffer(frame.encode());

        EmbeddedChannel channel = new EmbeddedChannel(new NettyDecoder());
        Assertions.assertFalse(channel.writeInbound(buf.readBytes(3)));
        Assertions.assertTrue(channel.writeInbound(buf));
        Assertions.assertTrue(channel.finish());

        SbpFrame outFrame = channel.readInbound();
        Assertions.assertEquals(frame, outFrame);
        Assertions.assertNull(channel.readInbound());
    }
}