package sdk.elastic.stream.client.netty;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import io.netty.channel.embedded.EmbeddedChannel;
import java.nio.ByteBuffer;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;
import sdk.elastic.stream.client.protocol.SbpFrame;
import sdk.elastic.stream.client.protocol.SbpFrameBuilder;

class NettyEncoderTest {

    @Test
    void testEncode() {
        SbpFrameBuilder defaultBuilder = SbpFrame.newBuilder()
            .setMagicCode((byte) 0x01)
            .setOperationCode((short) 0x02)
            .setFlag((byte) 0x03)
            .setStreamId(0x04);
        defaultBuilder.setHeader(ByteBuffer.wrap(new byte[] {0x05, 0x06, 0x07}));
        defaultBuilder.setPayload(new ByteBuffer[] {ByteBuffer.wrap(new byte[] {0x08, 0x09, 0x0a})});
        SbpFrame frame = defaultBuilder.build();
        ByteBuf buf = Unpooled.wrappedBuffer(frame.encode());

        EmbeddedChannel channel = new EmbeddedChannel(new NettyEncoder());
        Assertions.assertTrue(channel.writeOutbound(buf));
        Assertions.assertTrue(channel.finish());

        ByteBuf out = channel.readOutbound();
        Assertions.assertEquals(buf, out);
        Assertions.assertNull(channel.readOutbound());
    }
}