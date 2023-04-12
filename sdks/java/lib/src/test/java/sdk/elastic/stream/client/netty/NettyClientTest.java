package sdk.elastic.stream.client.netty;

import io.netty.util.HashedWheelTimer;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.concurrent.ExecutionException;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import sdk.elastic.stream.apis.ClientConfiguration;
import sdk.elastic.stream.apis.ClientConfigurationBuilder;
import sdk.elastic.stream.client.protocol.SbpFrame;
import sdk.elastic.stream.client.protocol.SbpFrameBuilder;
import sdk.elastic.stream.client.route.Address;

import static sdk.elastic.stream.client.netty.DefaultMockNettyServer.TEST_TIMEOUT_OPERATION_CODE;

class NettyClientTest {
    private static final int serverPort = 8100;
    private static final String defaultResponsePrefix = "hello, world: ";
    private static final Address defaultPmAddress = new Address("localhost", serverPort);
    private static final HashedWheelTimer timer = new HashedWheelTimer(r -> new Thread(r, "TestHouseKeepingService"));
    private static DefaultMockNettyServerThread serverThread;

    @BeforeAll
    static void setUp() throws InterruptedException {
        serverThread = new DefaultMockNettyServerThread(serverPort, defaultResponsePrefix);
        serverThread.start();
    }

    @Test
    void testInvokeAsync() throws Exception {
        String requestMessage = "This is Async test";
        ClientConfigurationBuilder builder = ClientConfiguration.newBuilder()
            .setPmEndpoint(String.format("%s:%d", defaultPmAddress.getHost(), defaultPmAddress.getPort()))
            .setConnectionTimeout(Duration.ofSeconds(3))
            .setChannelMaxIdleTime(Duration.ofSeconds(10));
        try (NettyClient client = new NettyClient(builder.build(), timer)) {
            client.start();
            SbpFrame sbpFrame = new SbpFrameBuilder()
                .setFlag(SbpFrame.GENERAL_RESPONSE_FLAG)
                .setPayload(new ByteBuffer[] {ByteBuffer.wrap(requestMessage.getBytes(StandardCharsets.ISO_8859_1))})
                .setOperationCode((short) 10)
                .build();

            SbpFrame responseFrame = client.invokeAsync(sbpFrame, Duration.ofSeconds(3)).get();

            Assertions.assertEquals(responseFrame.getFlag(), SbpFrame.GENERAL_RESPONSE_FLAG);
            Assertions.assertEquals(responseFrame.getStreamId(), 0L);
            byte[] payload = new byte[responseFrame.payloadLength()];
            int index = 0;
            for (ByteBuffer buffer : responseFrame.getPayload()) {
                buffer.get(payload, index, buffer.remaining());
                index += buffer.remaining();
            }
            String responseMessage = new String(payload, StandardCharsets.ISO_8859_1);
            Assertions.assertEquals(responseMessage, defaultResponsePrefix + requestMessage);
        }
    }

    @Test
    void testInvokeAsyncError() throws Exception {
        String requestMessage = "This is ASync Error test";
        ClientConfigurationBuilder builder = ClientConfiguration.newBuilder()
            .setPmEndpoint(String.format("%s:%d", defaultPmAddress.getHost(), defaultPmAddress.getPort()))
            .setConnectionTimeout(Duration.ofSeconds(3))
            .setChannelMaxIdleTime(Duration.ofSeconds(10));
        try (NettyClient client = new NettyClient(builder.build(), timer)) {
            client.start();
            // no operation code is set.
            SbpFrame sbpFrame = new SbpFrameBuilder()
                .setFlag(SbpFrame.GENERAL_RESPONSE_FLAG)
                .setPayload(new ByteBuffer[] {ByteBuffer.wrap(requestMessage.getBytes(StandardCharsets.ISO_8859_1))})
                .build();
            Assertions.assertThrows(ExecutionException.class, () -> client.invokeAsync(sbpFrame, Duration.ofSeconds(3)).get());
        }
    }

    @Test
    void testRequestTimeout() throws Exception {
        String requestMessage = "This is Timeout test";
        ClientConfigurationBuilder builder = ClientConfiguration.newBuilder()
            .setPmEndpoint(String.format("%s:%d", defaultPmAddress.getHost(), defaultPmAddress.getPort()))
            .setConnectionTimeout(Duration.ofSeconds(3))
            .setChannelMaxIdleTime(Duration.ofSeconds(10));
        try (NettyClient client = new NettyClient(builder.build(), timer)) {
            client.start();
            SbpFrame sbpFrame = new SbpFrameBuilder()
                .setFlag(SbpFrame.GENERAL_RESPONSE_FLAG)
                .setPayload(new ByteBuffer[] {ByteBuffer.wrap(requestMessage.getBytes(StandardCharsets.ISO_8859_1))})
                .setOperationCode(TEST_TIMEOUT_OPERATION_CODE)
                .build();
            Assertions.assertThrows(ExecutionException.class, () -> client.invokeAsync(sbpFrame, Duration.ofSeconds(3)).get());
        }
    }

    @AfterAll
    static void tearDown() throws IOException {
        serverThread.close();
    }
}