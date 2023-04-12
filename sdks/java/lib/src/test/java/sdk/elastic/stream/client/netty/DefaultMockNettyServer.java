package sdk.elastic.stream.client.netty;

import io.netty.bootstrap.ServerBootstrap;
import io.netty.channel.ChannelFuture;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelInitializer;
import io.netty.channel.ChannelOption;
import io.netty.channel.ChannelPipeline;
import io.netty.channel.EventLoopGroup;
import io.netty.channel.SimpleChannelInboundHandler;
import io.netty.channel.nio.NioEventLoopGroup;
import io.netty.channel.socket.SocketChannel;
import io.netty.channel.socket.nio.NioServerSocketChannel;
import io.netty.handler.logging.LogLevel;
import io.netty.handler.logging.LoggingHandler;
import java.io.Closeable;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import sdk.elastic.stream.client.protocol.SbpFrame;
import sdk.elastic.stream.client.protocol.SbpFrameBuilder;
import sdk.elastic.stream.models.OperationCode;

public class DefaultMockNettyServer extends NettyRemotingAbstract implements Closeable {
    public final static short TEST_TIMEOUT_OPERATION_CODE = 0x0FFF;
    private final EventLoopGroup bossGroup;
    private final EventLoopGroup workerGroup;
    private final int port;
    private final String responsePrefix;

    /**
     * Constructor, specifying capacity of one-way and asynchronous semaphores.
     *
     * @param permitsAsync Number of permits for asynchronous requests.
     */
    public DefaultMockNettyServer(int permitsAsync, int port, String responsePrefix) {
        super(permitsAsync);

        bossGroup = new NioEventLoopGroup(1);
        workerGroup = new NioEventLoopGroup();
        this.port = port;
        this.responsePrefix = responsePrefix;

    }

    public void start() throws InterruptedException {
        NettyEncoder sharedEncoder = new NettyEncoder();

        ServerBootstrap b = new ServerBootstrap();
        b.group(bossGroup, workerGroup)
            .channel(NioServerSocketChannel.class)
            .option(ChannelOption.SO_BACKLOG, 100)
            .handler(new LoggingHandler(LogLevel.INFO))
            .childHandler(new ChannelInitializer<SocketChannel>() {
                @Override
                public void initChannel(SocketChannel ch) throws Exception {
                    ChannelPipeline p = ch.pipeline();
                    p.addLast(sharedEncoder, new NettyDecoder(), new NettyServerHandler());
                }
            });

        // Start the server.
        ChannelFuture f = b.bind(port).sync();

        // Wait until the server socket is closed.
        f.channel().closeFuture().sync();
    }

    @Override
    public void close() throws IOException {
        // Shut down all event loops to terminate all threads.
        bossGroup.shutdownGracefully();
        workerGroup.shutdownGracefully();
    }

    class NettyServerHandler extends SimpleChannelInboundHandler<SbpFrame> {

        @Override
        protected void channelRead0(ChannelHandlerContext ctx, SbpFrame frame) throws Exception {
            // frame must have an operation code
            if (frame.getOperationCode() == 0) {
                ctx.writeAndFlush(new SbpFrameBuilder()
                    .setFlag(SbpFrame.ERROR_RESPONSE_FLAG)
                    .setStreamId(frame.getStreamId())
                    .build());
                return;
            }

            if (frame.getOperationCode() == TEST_TIMEOUT_OPERATION_CODE) {
                Thread.sleep(20000);
                return;
            }

            if (frame.getOperationCode() == OperationCode.PING.getCode()) {
                ctx.writeAndFlush(handlePing(frame));
                return;
            }

            ByteBuffer[] resultPayload = new ByteBuffer[frame.getPayload().length + 1];
            resultPayload[0] = ByteBuffer.wrap(responsePrefix.getBytes(StandardCharsets.ISO_8859_1));
            for (int i = 0; i < frame.getPayload().length; i++) {
                resultPayload[i + 1] = frame.getPayload()[i];
            }
            ctx.writeAndFlush(new SbpFrameBuilder()
                .setFlag(SbpFrame.GENERAL_RESPONSE_FLAG)
                .setPayload(resultPayload)
                .setHeader(frame.getHeader())
                .setStreamId(frame.getStreamId())
                .setOperationCode((short) 1)
                .build());
        }

        private SbpFrame handlePing(SbpFrame frame) {
            return new SbpFrameBuilder()
                .setFlag((byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG))
                .setPayload(frame.getPayload())
                .setHeader(frame.getHeader())
                .setStreamId(frame.getStreamId())
                .setOperationCode(frame.getOperationCode())
                .build();
        }
    }
}
