package sdk.elastic.stream.client.netty;

import io.netty.bootstrap.Bootstrap;
import io.netty.channel.Channel;
import io.netty.channel.ChannelDuplexHandler;
import io.netty.channel.ChannelFuture;
import io.netty.channel.ChannelFutureListener;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelInitializer;
import io.netty.channel.ChannelOption;
import io.netty.channel.ChannelPromise;
import io.netty.channel.EventLoopGroup;
import io.netty.channel.SimpleChannelInboundHandler;
import io.netty.channel.nio.NioEventLoopGroup;
import io.netty.channel.socket.SocketChannel;
import io.netty.channel.socket.nio.NioSocketChannel;
import io.netty.handler.timeout.IdleState;
import io.netty.handler.timeout.IdleStateEvent;
import io.netty.handler.timeout.IdleStateHandler;
import io.netty.util.HashedWheelTimer;
import io.netty.util.Timeout;
import io.netty.util.TimerTask;
import java.io.IOException;
import java.net.SocketAddress;
import java.time.Duration;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentMap;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import sdk.elastic.stream.apis.ClientConfiguration;
import sdk.elastic.stream.apis.exception.ClientException;
import sdk.elastic.stream.client.RemotingClient;
import sdk.elastic.stream.client.common.ClientId;
import sdk.elastic.stream.client.common.RemotingUtil;
import sdk.elastic.stream.client.common.RequestIdGenerator;
import sdk.elastic.stream.client.misc.ThreadFactoryImpl;
import sdk.elastic.stream.client.protocol.RemotingItem;
import sdk.elastic.stream.client.protocol.SbpFrame;
import sdk.elastic.stream.client.route.Address;
import sdk.elastic.stream.client.route.Endpoints;

public class NettyClient extends NettyRemotingAbstract implements RemotingClient {
    private static final Logger log = LoggerFactory.getLogger(NettyClient.class);
    private static final long LOCK_TIMEOUT_MILLIS = 3000;

    protected final ClientId clientId;
    protected final ClientConfiguration clientConfiguration;
    protected Address pmAddress;
    private final Bootstrap bootstrap = new Bootstrap();
    private final EventLoopGroup eventLoopGroupWorker;
    private final Lock lockChannelTables = new ReentrantLock();
    private final Lock pmAddressUpdateLock = new ReentrantLock();
    private final ConcurrentMap<Address, ChannelWrapper> channelTables = new ConcurrentHashMap<>();
    private final HashedWheelTimer timer;
    /**
     * Since a netty client has separate input channels and output channels, one RequestIdGenerator is created for one client rather than one channel.
     */
    private final RequestIdGenerator requestIdGenerator;

    public NettyClient(final ClientConfiguration clientConfiguration, HashedWheelTimer timer) {
        this(clientConfiguration, timer, null);
    }

    public NettyClient(final ClientConfiguration clientConfiguration,
        HashedWheelTimer timer, final EventLoopGroup eventLoopGroup) {
        super(clientConfiguration.getClientAsyncSemaphoreValue());
        this.clientConfiguration = clientConfiguration;
        this.requestIdGenerator = new RequestIdGenerator();
        this.timer = timer;

        this.pmAddress = new Endpoints(clientConfiguration.getPlacementManagerEndpoint()).getAddresses().get(0);
        this.clientId = new ClientId();
        this.eventLoopGroupWorker = eventLoopGroup == null ? new NioEventLoopGroup(1,
            new ThreadFactoryImpl("ClientEventLoopGroupWorker", clientId.getIndex())) : eventLoopGroup;
    }

    @Override
    public ClientId getClientId() {
        return clientId;
    }

    @Override
    public void start() throws Exception {
        NettyEncoder sharedEncoder = new NettyEncoder();
        this.bootstrap.group(this.eventLoopGroupWorker).channel(NioSocketChannel.class)
            .option(ChannelOption.TCP_NODELAY, true)
            .option(ChannelOption.SO_KEEPALIVE, false)
            .option(ChannelOption.CONNECT_TIMEOUT_MILLIS, (int) clientConfiguration.getConnectionTimeout().toMillis())
            .handler(new ChannelInitializer<SocketChannel>() {
                @Override
                public void initChannel(SocketChannel ch) throws Exception {
                    ch.pipeline().addLast(
                        sharedEncoder,
                        new NettyDecoder(),
                        new IdleStateHandler(0, 0, (int) clientConfiguration.getChannelMaxIdleTime().getSeconds()),
                        new NettyConnectManageHandler(),
                        new NettyClientHandler());
                }
            });

        TimerTask timerTaskScanResponseTable = new TimerTask() {
            @Override
            public void run(Timeout timeout) {
                try {
                    NettyClient.this.purifyResponseTable();
                } catch (Throwable e) {
                    log.error("purifyResponseTable exception", e);
                } finally {
                    timer.newTimeout(this, 1, TimeUnit.SECONDS);
                }
            }
        };
        this.timer.newTimeout(timerTaskScanResponseTable, 3, TimeUnit.SECONDS);
    }

    public CompletableFuture<SbpFrame> invokeAsync(RemotingItem request, Duration timeout) {
        return invokeAsync(pmAddress, request, timeout);
    }

    public void updatePmAddress(Address address) {
        try {
            if (pmAddressUpdateLock.tryLock(LOCK_TIMEOUT_MILLIS, TimeUnit.MILLISECONDS)) {
                try {
                    if (!pmAddress.equals(address)) {
                        log.info("update pm address from {} to {}", pmAddress, address);
                        pmAddress = address;
                    }
                } catch (Exception e) {
                    log.error("update pm address error", e);
                } finally {
                    pmAddressUpdateLock.unlock();
                }
            } else {
                log.error("update pm address timeout");
            }
        } catch (InterruptedException e) {
            log.warn("update pm address interrupted", e);
        }
    }

    @Override
    public CompletableFuture<SbpFrame> invokeAsync(Address address, RemotingItem request, Duration timeout) {
        if (timeout.isNegative() || timeout.isZero()) {
            CompletableFuture<SbpFrame> responseFuture = new CompletableFuture<>();
            responseFuture.completeExceptionally(new ClientException("timeout must be positive"));
            return responseFuture;
        }

        return this.getOrCreateChannel(address).thenCompose(channel -> {
            CompletableFuture<SbpFrame> responseFuture = new CompletableFuture<>();
            if (channel != null && channel.isActive()) {
                SbpFrame requestFrame = (SbpFrame) request;
                requestFrame.setStreamId(this.requestIdGenerator.getId());
                super.invokeAsyncImpl(channel, requestFrame, timeout.toMillis(), responseFuture);
            } else {
                this.closeChannel(address, channel);
                responseFuture.completeExceptionally(new ClientException("netty channel not available"));
            }
            return responseFuture;
        });
    }

    @Override
    public void close() throws IOException {
        try {
            for (Address address : this.channelTables.keySet()) {
                this.closeChannel(address, this.channelTables.get(address).getChannel());
            }

            this.channelTables.clear();

            this.eventLoopGroupWorker.shutdownGracefully();
        } catch (Exception e) {
            log.error("NettyRemotingClient shutdown exception, ", e);
        }
    }

    public void closeChannel(final Address address, final Channel channel) {
        if (null == channel) {
            return;
        }

        final Address addressRemote = null == address ? RemotingUtil.parseChannelRemoteAddress(channel) : address;
        assert addressRemote != null;

        try {
            if (this.lockChannelTables.tryLock(LOCK_TIMEOUT_MILLIS, TimeUnit.MILLISECONDS)) {
                try {
                    boolean removeItemFromTable = true;
                    final ChannelWrapper prevCW = this.channelTables.get(addressRemote);

                    log.info("closeChannel: begin close the channel[{}] Found: {}", addressRemote, prevCW != null);

                    if (null == prevCW) {
                        log.info("closeChannel: the channel[{}] has been removed from the channel table before", addressRemote);
                        removeItemFromTable = false;
                    } else if (prevCW.getChannel() != channel) {
                        log.info("closeChannel: the channel[{}] has been closed before, and has been created again, nothing to do.",
                            addressRemote);
                        removeItemFromTable = false;
                    }

                    if (removeItemFromTable) {
                        this.channelTables.remove(addressRemote);
                        log.info("closeChannel: the channel[{}] was removed from channel table", addressRemote);
                    }

                    RemotingUtil.closeChannel(channel);
                } catch (Exception e) {
                    log.error("closeChannel: close the channel exception", e);
                } finally {
                    this.lockChannelTables.unlock();
                }
            } else {
                log.warn("closeChannel: try to lock channel table, but timeout, {}ms", LOCK_TIMEOUT_MILLIS);
            }
        } catch (InterruptedException e) {
            log.error("closeChannel exception", e);
        }
    }

    public void closeChannel(final Channel channel) {
        if (null == channel) {
            return;
        }

        try {
            if (this.lockChannelTables.tryLock(LOCK_TIMEOUT_MILLIS, TimeUnit.MILLISECONDS)) {
                try {
                    boolean removeItemFromTable = true;
                    ChannelWrapper prevCW = null;
                    Address addrRemote = null;
                    for (Map.Entry<Address, ChannelWrapper> entry : channelTables.entrySet()) {
                        Address key = entry.getKey();
                        ChannelWrapper prev = entry.getValue();
                        if (prev.getChannel() != null) {
                            if (prev.getChannel() == channel) {
                                prevCW = prev;
                                addrRemote = key;
                                break;
                            }
                        }
                    }

                    if (null == prevCW) {
                        log.info("eventCloseChannel: the channel[{}] has been removed from the channel table before", addrRemote);
                        removeItemFromTable = false;
                    }

                    if (removeItemFromTable) {
                        this.channelTables.remove(addrRemote);
                        log.info("closeChannel: the channel[{}] was removed from channel table", addrRemote);
                        RemotingUtil.closeChannel(channel);
                    }
                } catch (Exception e) {
                    log.error("closeChannel: close the channel exception", e);
                } finally {
                    this.lockChannelTables.unlock();
                }
            } else {
                log.warn("closeChannel: try to lock channel table, but timeout, {}ms", LOCK_TIMEOUT_MILLIS);
            }
        } catch (InterruptedException e) {
            log.error("closeChannel exception", e);
        }
    }

    private CompletableFuture<Channel> getOrCreateChannel(final Address address) {
        CompletableFuture<Channel> completableFuture = new CompletableFuture<>();
        ChannelWrapper cw = this.channelTables.get(address);
        if (cw != null && cw.isOK()) {
            completableFuture.complete(cw.getChannel());
            return completableFuture;
        }

        try {
            if (this.lockChannelTables.tryLock(LOCK_TIMEOUT_MILLIS, TimeUnit.MILLISECONDS)) {
                try {
                    boolean createNewConnection;
                    cw = this.channelTables.get(address);
                    if (cw != null) {

                        if (cw.isOK()) {
                            completableFuture.complete(cw.getChannel());
                            return completableFuture;
                        } else if (!cw.getChannelFuture().isDone()) {
                            createNewConnection = false;
                        } else {
                            this.channelTables.remove(address);
                            createNewConnection = true;
                        }
                    } else {
                        createNewConnection = true;
                    }

                    if (createNewConnection) {
                        ChannelFuture channelFuture = bootstrap
                            .connect(address.getHost(), address.getPort());
                        log.info("createChannel: begin to connect remote host[{}] asynchronously", address);
                        cw = new ChannelWrapper(channelFuture);
                        this.channelTables.put(address, cw);
                    }

                } catch (Exception e) {
                    log.error("createChannel: create channel exception", e);
                    completableFuture.completeExceptionally(e);
                    return completableFuture;
                } finally {
                    this.lockChannelTables.unlock();
                }
            } else {
                log.warn("createChannel: try to lock channel table, but timeout, {}ms", LOCK_TIMEOUT_MILLIS);
            }
        } catch (InterruptedException e) {
            log.error("interrupted when creating channel: {}", e.getMessage());
        }

        if (cw != null) {
            ChannelFuture channelFuture = cw.getChannelFuture();
            channelFuture.addListener((ChannelFutureListener) future -> {
                if (future.isSuccess()) {
                    log.info("createChannel: connect remote host[{}] success, {}", address, channelFuture);
                    completableFuture.complete(future.channel());
                } else {
                    log.warn("createChannel: connect remote host[" + address + "] failed, " + channelFuture);
                    completableFuture.completeExceptionally(future.cause());
                }
            });
        }

        return completableFuture;
    }

    class NettyClientHandler extends SimpleChannelInboundHandler<SbpFrame> {

        @Override
        protected void channelRead0(ChannelHandlerContext ctx, SbpFrame frame) throws Exception {
            processMessageReceived(ctx, frame);
        }
    }

    static class ChannelWrapper {
        private final ChannelFuture channelFuture;
        // affect sync or async request.
        private long lastResponseTime;

        public ChannelWrapper(ChannelFuture channelFuture) {
            this.channelFuture = channelFuture;
            this.lastResponseTime = System.currentTimeMillis();
        }

        public boolean isOK() {
            return this.channelFuture.channel() != null && this.channelFuture.channel().isActive();
        }

        public boolean isWritable() {
            return this.channelFuture.channel().isWritable();
        }

        private Channel getChannel() {
            return this.channelFuture.channel();
        }

        public ChannelFuture getChannelFuture() {
            return channelFuture;
        }

        public long getLastResponseTime() {
            return this.lastResponseTime;
        }

        public void updateLastResponseTime() {
            this.lastResponseTime = System.currentTimeMillis();
        }
    }

    class NettyConnectManageHandler extends ChannelDuplexHandler {
        @Override
        public void connect(ChannelHandlerContext ctx, SocketAddress remoteAddress, SocketAddress localAddress,
            ChannelPromise promise) throws Exception {
            final String local = localAddress == null ? "UNKNOWN" : RemotingUtil.parseSocketAddressAddr(localAddress);
            final String remote = remoteAddress == null ? "UNKNOWN" : RemotingUtil.parseSocketAddressAddr(remoteAddress);
            log.info("NETTY CLIENT PIPELINE: CONNECT  {} => {}", local, remote);

            super.connect(ctx, remoteAddress, localAddress, promise);
        }

        @Override
        public void disconnect(ChannelHandlerContext ctx, ChannelPromise promise) throws Exception {
            final String remoteAddress = RemotingUtil.parseChannelRemoteAddr(ctx.channel());
            log.info("NETTY CLIENT PIPELINE: DISCONNECT {}", remoteAddress);
            closeChannel(ctx.channel());
            super.disconnect(ctx, promise);
        }

        @Override
        public void close(ChannelHandlerContext ctx, ChannelPromise promise) throws Exception {
            final String remoteAddress = RemotingUtil.parseChannelRemoteAddr(ctx.channel());
            log.info("NETTY CLIENT PIPELINE: CLOSE {}", remoteAddress);
            closeChannel(ctx.channel());
            super.close(ctx, promise);
            NettyClient.this.failFast(ctx.channel());
        }

        @Override
        public void userEventTriggered(ChannelHandlerContext ctx, Object evt) {
            if (evt instanceof IdleStateEvent) {
                IdleStateEvent event = (IdleStateEvent) evt;
                if (event.state().equals(IdleState.ALL_IDLE)) {
                    final String remoteAddress = RemotingUtil.parseChannelRemoteAddr(ctx.channel());
                    log.warn("NETTY CLIENT PIPELINE: IDLE exception [{}]", remoteAddress);
                    closeChannel(ctx.channel());
                }
            }

            ctx.fireUserEventTriggered(evt);
        }

        @Override
        public void exceptionCaught(ChannelHandlerContext ctx, Throwable cause) throws Exception {
            final String remoteAddress = RemotingUtil.parseChannelRemoteAddr(ctx.channel());
            log.warn("NETTY CLIENT PIPELINE: exceptionCaught {}", remoteAddress);
            log.warn("NETTY CLIENT PIPELINE: exceptionCaught exception.", cause);
            closeChannel(ctx.channel());
        }
    }
}
