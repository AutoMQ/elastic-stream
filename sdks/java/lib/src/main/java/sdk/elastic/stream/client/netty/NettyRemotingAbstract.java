/*
 * Licensed to the Apache Software Foundation (ASF) under one or more
 * contributor license agreements.  See the NOTICE file distributed with
 * this work for additional information regarding copyright ownership.
 * The ASF licenses this file to You under the Apache License, Version 2.0
 * (the "License"); you may not use this file except in compliance with
 * the License.  You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package sdk.elastic.stream.client.netty;

import io.netty.channel.Channel;
import io.netty.channel.ChannelFutureListener;
import io.netty.channel.ChannelHandlerContext;
import java.util.Iterator;
import java.util.LinkedList;
import java.util.List;
import java.util.Map.Entry;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ConcurrentMap;
import java.util.concurrent.Semaphore;
import java.util.concurrent.TimeUnit;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import sdk.elastic.stream.apis.exception.ClientException;
import sdk.elastic.stream.client.common.RemotingUtil;
import sdk.elastic.stream.client.common.RequestIdGenerator;
import sdk.elastic.stream.client.common.SemaphoreReleaseOnlyOnce;
import sdk.elastic.stream.client.protocol.SbpFrame;

public abstract class NettyRemotingAbstract {

    /**
     * Remoting logger instance.
     */
    private static final Logger log = LoggerFactory.getLogger(NettyRemotingAbstract.class);
    /**
     * Generate stream id for each request.
     */
    private final RequestIdGenerator requestIdGenerator;

    /**
     * Semaphore to limit maximum number of on-going asynchronous requests, which protects system memory footprint.
     */
    protected final Semaphore semaphoreAsync;

    /**
     * This map caches all on-going requests.
     */
    protected final ConcurrentMap<Integer /* opaque */, ResponseFuture> responseTable =
        new ConcurrentHashMap<>(256);

    /**
     * This map caches all alive channels based on responseTable.
     */
    public final ConcurrentMap<Channel, Boolean> aliveChannelTable = new ConcurrentHashMap<>(256);

    /**
     * Constructor, specifying capacity of one-way and asynchronous semaphores.
     *
     * @param permitsAsync Number of permits for asynchronous requests.
     */
    public NettyRemotingAbstract(final int permitsAsync) {
        this.semaphoreAsync = new Semaphore(permitsAsync, true);
        this.requestIdGenerator = new RequestIdGenerator();
    }

    /**
     * Entry of incoming SbpFrame processing.
     *
     * @param ctx   Channel handler context.
     * @param frame incoming SbpFrame.
     */
    public void processMessageReceived(ChannelHandlerContext ctx, SbpFrame frame) {
        if (frame == null) {
            return;
        }
        if (frame.isRequest()) {
            processRequestSbpFrame(ctx, frame);
        } else {
            processResponseSbpFrame(ctx, frame);
        }
    }

    /**
     * Process incoming request SbpFrame issued by remote peer.
     *
     * @param ctx   channel handler context.
     * @param frame request SbpFrame.
     */
    public void processRequestSbpFrame(final ChannelHandlerContext ctx, final SbpFrame frame) {

    }

    /**
     * Process response from remote peer to the previous issued requests.
     *
     * @param ctx   channel handler context.
     * @param frame response SbpFrame instance.
     */
    public void processResponseSbpFrame(ChannelHandlerContext ctx, SbpFrame frame) {
        final int opaque = frame.getId();

        // TODO: multiple responses may be received for just one request. Therefore, response flags have to be handled.
        final ResponseFuture responseFuture = responseTable.get(opaque);
        if (responseFuture != null) {
            if ((frame.getFlag() & SbpFrame.ERROR_RESPONSE_FLAG) != 0) {
                log.error("receive error response from:{}, requestId {}.", RemotingUtil.parseChannelRemoteAddr(ctx.channel()), opaque);
                responseFuture.getCompletableFuture().completeExceptionally(new ClientException("response error frame"));
            } else {
                responseFuture.getCompletableFuture().complete(frame);
            }
            responseTable.remove(opaque);
            responseFuture.release();
        } else {
            log.warn("receive response, but not matched any request, " + RemotingUtil.parseChannelRemoteAddr(ctx.channel()));
            log.warn(frame.toString());
        }
    }

    /**
     * <p>
     * This method is periodically invoked to scan and expire deprecated request.
     * </p>
     */
    public void purifyResponseTable() {
        final List<ResponseFuture> rfList = new LinkedList<>();
        Iterator<Entry<Integer, ResponseFuture>> it = this.responseTable.entrySet().iterator();
        while (it.hasNext()) {
            Entry<Integer, ResponseFuture> next = it.next();
            ResponseFuture rep = next.getValue();

            if ((rep.getBeginTimestamp() + rep.getTimeoutMillis() + 1000) <= System.currentTimeMillis()) {
                rep.release();
                it.remove();
                aliveChannelTable.remove(rep.getChannel());
                rfList.add(rep);
                log.warn("remove timeout request, " + rep);
            } else {
                aliveChannelTable.put(rep.getChannel(), true);
            }
        }

        for (ResponseFuture rf : rfList) {
            rf.getCompletableFuture().completeExceptionally(new ClientException("timeout waiting for response"));
        }
    }

    public CompletableFuture<SbpFrame> invokeAsyncImpl(final Channel channel, final SbpFrame request,
        final long timeoutMillis,
        final CompletableFuture<SbpFrame> future) {
        try {
            boolean acquired = this.semaphoreAsync.tryAcquire(timeoutMillis, TimeUnit.MILLISECONDS);
            if (acquired) {
                final int opaque = this.requestIdGenerator.getId();
                request.setStreamId(opaque);
                final SemaphoreReleaseOnlyOnce once = new SemaphoreReleaseOnlyOnce(this.semaphoreAsync);
                final ResponseFuture responseFuture = new ResponseFuture(channel, opaque, timeoutMillis, future, once);

                this.responseTable.put(opaque, responseFuture);
                channel.writeAndFlush(request).addListener((ChannelFutureListener) f -> {
                    if (f.isSuccess()) {
                        responseFuture.setSendRequestOK(true);
                        return;
                    }
                    requestFail(opaque);
                    log.warn("send a request SbpFrame to channel <{}> failed. Error Msg: {}", RemotingUtil.parseChannelRemoteAddr(channel), f.cause().getMessage());
                });
            } else {
                if (timeoutMillis <= 0) {
                    String info = "invokeAsyncImpl invoke too fast";
                    log.error(info);
                    future.completeExceptionally(new ClientException(info));
                    return future;
                } else {
                    String info =
                        String.format("invokeAsyncImpl tryAcquire semaphore timeout, %dms, waiting thread nums: %d semaphoreAsyncValue: %d",
                            timeoutMillis,
                            this.semaphoreAsync.getQueueLength(),
                            this.semaphoreAsync.availablePermits()
                        );
                    log.warn(info);
                    future.completeExceptionally(new ClientException(info));
                    return future;
                }
            }

        } catch (Throwable ex) {
            future.completeExceptionally(ex);
        }

        return future;
    }

    /**
     * Send failure response immediately due to channel problem.
     * @param opaque unique id of responseFuture.
     */
    private void requestFail(final int opaque) {
        ResponseFuture responseFuture = responseTable.remove(opaque);
        if (responseFuture != null) {
            responseFuture.setSendRequestOK(false);
            responseFuture.getCompletableFuture().completeExceptionally(new ClientException("fail fast due to channel problem"));
            responseFuture.release();
        }
    }

    /**
     * mark the request of the specified channel as fail and to invoke fail callback immediately
     *
     * @param channel the channel which is close already
     */
    protected void failFast(final Channel channel) {
        for (Entry<Integer, ResponseFuture> entry : responseTable.entrySet()) {
            if (entry.getValue().getChannel() == channel) {
                Integer opaque = entry.getKey();
                if (opaque != null) {
                    requestFail(opaque);
                }
            }
        }
    }
}
