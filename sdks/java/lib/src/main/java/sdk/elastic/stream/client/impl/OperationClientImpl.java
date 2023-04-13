package sdk.elastic.stream.client.impl;

import com.google.common.base.Preconditions;
import com.google.common.cache.CacheLoader;
import com.google.flatbuffers.FlatBufferBuilder;
import io.netty.channel.Channel;
import io.netty.util.HashedWheelTimer;
import io.netty.util.Timeout;
import io.netty.util.TimerTask;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.time.Duration;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import sdk.elastic.stream.apis.ClientConfiguration;
import sdk.elastic.stream.apis.OperationClient;
import sdk.elastic.stream.apis.exception.ClientException;
import sdk.elastic.stream.apis.manager.ResourceManager;
import sdk.elastic.stream.client.common.ClientId;
import sdk.elastic.stream.client.common.PmUtil;
import sdk.elastic.stream.client.common.ProtocolUtil;
import sdk.elastic.stream.client.common.RemotingUtil;
import sdk.elastic.stream.client.netty.NettyClient;
import sdk.elastic.stream.client.protocol.SbpFrame;
import sdk.elastic.stream.client.route.Address;
import sdk.elastic.stream.flatc.header.AppendInfoT;
import sdk.elastic.stream.flatc.header.AppendRequest;
import sdk.elastic.stream.flatc.header.AppendRequestT;
import sdk.elastic.stream.flatc.header.AppendResponse;
import sdk.elastic.stream.flatc.header.AppendResultT;
import sdk.elastic.stream.flatc.header.ClientRole;
import sdk.elastic.stream.flatc.header.CreateStreamResultT;
import sdk.elastic.stream.flatc.header.FetchInfoT;
import sdk.elastic.stream.flatc.header.FetchRequest;
import sdk.elastic.stream.flatc.header.FetchRequestT;
import sdk.elastic.stream.flatc.header.FetchResponse;
import sdk.elastic.stream.flatc.header.FetchResultT;
import sdk.elastic.stream.flatc.header.HeartbeatRequest;
import sdk.elastic.stream.flatc.header.HeartbeatRequestT;
import sdk.elastic.stream.flatc.header.HeartbeatResponse;
import sdk.elastic.stream.flatc.header.RangeCriteriaT;
import sdk.elastic.stream.flatc.header.RangeIdT;
import sdk.elastic.stream.flatc.header.RangeT;
import sdk.elastic.stream.flatc.header.SealRangesResultT;
import sdk.elastic.stream.flatc.header.StreamT;
import sdk.elastic.stream.models.OperationCode;
import sdk.elastic.stream.models.RangeTDecorator;
import sdk.elastic.stream.models.RecordBatch;

import static sdk.elastic.stream.flatc.header.ErrorCode.DN_NOT_LEADER_RANGE;
import static sdk.elastic.stream.flatc.header.ErrorCode.NO_NEW_RECORD;
import static sdk.elastic.stream.flatc.header.ErrorCode.OFFSET_OUT_OF_RANGE_BOUNDS;
import static sdk.elastic.stream.flatc.header.ErrorCode.OFFSET_OVERFLOW;
import static sdk.elastic.stream.flatc.header.ErrorCode.OK;
import static sdk.elastic.stream.flatc.header.ErrorCode.PM_INTERNAL_SERVER_ERROR;
import static sdk.elastic.stream.flatc.header.ErrorCode.RANGE_ALREADY_SEALED;
import static sdk.elastic.stream.flatc.header.ErrorCode.RANGE_NOT_FOUND;

public class OperationClientImpl implements OperationClient {
    private static final Logger log = LoggerFactory.getLogger(OperationClientImpl.class);
    private final NettyClient nettyClient;
    private final ResourceManager resourceManager;
    private final StreamRangeCache streamRangeCache;
    private final Duration heartbeatInterval;
    private final HashedWheelTimer timer = new HashedWheelTimer(r -> new Thread(r, "HouseKeepingService"));

    private static final Duration CACHE_LOAD_TIMEOUT = Duration.ofSeconds(3);

    protected OperationClientImpl(Duration heartbeatInterval, NettyClient nettyClient,
        ResourceManager resourceManager, StreamRangeCache streamRangeCache) {
        this.heartbeatInterval = heartbeatInterval;
        this.nettyClient = nettyClient;
        this.resourceManager = resourceManager;
        this.streamRangeCache = streamRangeCache;
    }

    public OperationClientImpl(ClientConfiguration clientConfiguration) {
        this.heartbeatInterval = clientConfiguration.getHeartbeatInterval();
        this.nettyClient = new NettyClient(clientConfiguration, timer);
        this.resourceManager = new ResourceManagerImpl(this.nettyClient);
        this.streamRangeCache = new StreamRangeCache(
            new CacheLoader<Long, TreeMap<Long, RangeT>>() {
                @Override
                public TreeMap<Long, RangeT> load(Long streamId) {
                    TreeMap<Long, RangeT> rangesMap = new TreeMap<>();
                    for (RangeT rangeT : fetchRangeArrayBasedOnStreamId(streamId).join()) {
                        rangesMap.put(rangeT.getStartOffset(), rangeT);
                    }
                    return rangesMap;
                }
            });
    }

    @Override
    public CompletableFuture<List<CreateStreamResultT>> createStreams(List<StreamT> streams, Duration timeout) {
        return this.resourceManager.createStreams(streams, timeout)
            .thenApply(list -> {
                for (CreateStreamResultT resultT : list) {
                    if (resultT.getStatus().getCode() == OK) {
                        this.streamRangeCache.put(resultT.getStream().getStreamId(), new RangeT[] {resultT.getRange()});
                    }
                }
                return list;
            });
    }

    @Override
    public CompletableFuture<AppendResultT> appendBatch(RecordBatch recordBatch, Duration timeout) {
        Preconditions.checkArgument(recordBatch != null && recordBatch.getRecords().size() > 0, "Invalid recordBatch since no records were found.");
        long streamId = recordBatch.getBatchMeta().getStreamId();

        return appendBatch0(recordBatch, timeout).thenCompose(appendResultT -> {
            short errorCode = appendResultT.getStatus().getCode();
            if (errorCode == OK) {
                return CompletableFuture.completedFuture(appendResultT);
            } else if (errorCode == RANGE_ALREADY_SEALED || errorCode == DN_NOT_LEADER_RANGE) {
                log.info("get errorCode {}. refresh cache and try again.", errorCode);
                // Since the cache is already outdated, we need to invalid the cache and try again.
                this.streamRangeCache.invalidate(streamId);
                return appendBatch0(recordBatch, timeout);
            }

            log.error("Got server error when appending a batch for streamId {}. Try to seal it. Error code: {}, msg: {} ", streamId, errorCode, appendResultT.getStatus().getMessage());
            RangeIdT rangeIdT = new RangeIdT();
            rangeIdT.setStreamId(streamId);
            rangeIdT.setRangeIndex(0);
            // seal the range and try again.
            return resourceManager.sealRanges(Collections.singletonList(rangeIdT), timeout)
                .thenCompose(sealResultList -> {
                    log.info("sealed for streamId {}, result: {}", streamId, sealResultList.get(0));
                    return handleSealRangesResultList(sealResultList).thenCompose(v -> appendBatch0(recordBatch, timeout));
                });
        });
    }

    @Override
    public CompletableFuture<Long> getLastWritableOffset(long streamId, Duration timeout) {
        return getLastWritableOffset0(streamId, timeout).thenCompose(rangeT -> {
            // The requested range has been sealed, so we have to invalidate the cache and try again.
            if (rangeT.getEndOffset() > 0) {
                this.streamRangeCache.invalidate(streamId);
                return getLastWritableOffset0(streamId, timeout).thenApply(RangeT::getNextOffset);
            }
            return CompletableFuture.completedFuture(rangeT.getNextOffset());
        });
    }

    @Override
    public CompletableFuture<List<RecordBatch>> fetchBatches(long streamId, long startOffset, int minBytes,
        int maxBytes, Duration timeout) {
        // Batches will be fetched from the primary node. If the primary node is unavailable, the next node will be accessed.
        return fetchBatches0(streamId, startOffset, minBytes, maxBytes, timeout, 2, -1);
    }

    @Override
    public CompletableFuture<Boolean> heartbeat(Address address, Duration timeout) {
        HeartbeatRequestT heartbeatRequestT = new HeartbeatRequestT();
        heartbeatRequestT.setClientId(getClientId().toString());
        heartbeatRequestT.setClientRole(ClientRole.CLIENT_ROLE_CUSTOMER);
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int heartbeatRequestOffset = HeartbeatRequest.pack(builder, heartbeatRequestT);
        builder.finish(heartbeatRequestOffset);

        SbpFrame sbpFrame = ProtocolUtil.constructRequestSbpFrame(OperationCode.HEARTBEAT, builder.dataBuffer());
        return nettyClient.invokeAsync(address, sbpFrame, timeout)
            .thenCompose(responseFrame -> {
                HeartbeatResponse response = HeartbeatResponse.getRootAsHeartbeatResponse(responseFrame.getHeader());
                Address updatePmAddress = PmUtil.extractNewPmAddress(response.status());
                // need to connect to new Pm primary node.
                if (updatePmAddress != null) {
                    nettyClient.updatePmAddress(updatePmAddress);
                    return nettyClient.invokeAsync(address, sbpFrame, timeout).thenCompose(responseFrame2 -> extractResponse(HeartbeatResponse.getRootAsHeartbeatResponse(responseFrame2.getHeader())));
                }

                return extractResponse(response);
            });
    }

    @Override
    public ClientId getClientId() {
        return this.nettyClient.getClientId();
    }

    @Override
    public void start() throws Exception {
        this.nettyClient.start();

        TimerTask timerTaskHeartBeat = new TimerTask() {
            @Override
            public void run(Timeout timeout) {
                try {
                    OperationClientImpl.this.keepNettyChannelsAlive();
                } catch (Throwable e) {
                    log.error("heartbeat exception ", e);
                } finally {
                    timer.newTimeout(this, OperationClientImpl.this.heartbeatInterval.getSeconds(), TimeUnit.SECONDS);
                }
            }
        };
        this.timer.newTimeout(timerTaskHeartBeat, this.heartbeatInterval.getSeconds(), TimeUnit.SECONDS);
    }

    private CompletableFuture<AppendResultT> appendBatch0(RecordBatch recordBatch, Duration timeout) {
        long streamId = recordBatch.getBatchMeta().getStreamId();

        return this.streamRangeCache.getLastRange(streamId).thenCompose(rangeT -> {
            ByteBuffer encodedBuffer = recordBatch.encode();

            AppendInfoT appendInfoT = new AppendInfoT();
            appendInfoT.setRequestIndex(0);
            appendInfoT.setBatchLength(encodedBuffer.remaining());
            appendInfoT.setRange(rangeT);
            AppendRequestT appendRequestT = new AppendRequestT();
            appendRequestT.setTimeoutMs((int) timeout.toMillis());
            appendRequestT.setAppendRequests(new AppendInfoT[] {appendInfoT});
            FlatBufferBuilder builder = new FlatBufferBuilder();
            int appendRequestOffset = AppendRequest.pack(builder, appendRequestT);
            builder.finish(appendRequestOffset);

            SbpFrame sbpFrame = ProtocolUtil.constructRequestSbpFrame(OperationCode.APPEND, builder.dataBuffer(), new ByteBuffer[] {encodedBuffer});

            RangeTDecorator rangeTDecorator = new RangeTDecorator(rangeT);
            Address targetDataNode = Address.fromAddress(rangeTDecorator.getPrimaryNode().getAdvertiseAddr());
            log.debug("trying to append a batch for streamId {} to datanode {}", streamId, targetDataNode.getAddress());

            return nettyClient.invokeAsync(targetDataNode, sbpFrame, timeout)
                .thenCompose(responseFrame -> {
                    AppendResponse response = AppendResponse.getRootAsAppendResponse(responseFrame.getHeader());
                    return extractResponse(response);
                }).thenCompose(appendResultList -> {
                    CompletableFuture<AppendResultT> future = new CompletableFuture<>();
                    short errorCode = appendResultList.get(0).getStatus().getCode();
                    // get client error code.
                    if (errorCode > OK && errorCode < PM_INTERNAL_SERVER_ERROR) {
                        future.completeExceptionally(new ClientException("Append batch failed with code " + errorCode + ", msg: " + appendResultList.get(0).getStatus().getMessage()));
                    } else {
                        future.complete(appendResultList.get(0));
                    }
                    return future;
                });
        });
    }

    private CompletableFuture<RangeT> getLastWritableOffset0(long streamId, Duration timeout) {
        return this.streamRangeCache.getLastRange(streamId).thenCompose(rangeT -> {
            RangeIdT rangeIdT = new RangeIdT();
            rangeIdT.setStreamId(rangeT.getStreamId());
            rangeIdT.setRangeIndex(rangeT.getRangeIndex());
            log.debug("trying to get last writable offset for streamId {}, range index {}", streamId, rangeIdT.getRangeIndex());
            // Any data node is ok.
            Address dataNodeAddress = Address.fromAddress(rangeT.getReplicaNodes()[0].getDataNode().getAdvertiseAddr());
            // The nextOffset in the cache may be out of date, so we need to fetch the latest range info from the PM.
            return this.resourceManager.describeRanges(dataNodeAddress, Collections.singletonList(rangeIdT), timeout)
                .thenCompose(list -> {
                    CompletableFuture<RangeT> future = new CompletableFuture<>();
                    if (list.get(0).getStatus().getCode() != OK) {
                        future.completeExceptionally(new ClientException("Get last writableOffset failed with code " + list.get(0).getStatus().getCode() + ", msg: " + list.get(0).getStatus().getMessage()));
                        return future;
                    }
                    future.complete(list.get(0).getRange());
                    return future;
                });
        });
    }

    /**
     * Fetch batches from data nodes with retry.
     *
     * @param streamId      stream id
     * @param startOffset   start offset
     * @param minBytes      min bytes to fetch
     * @param maxBytes      max bytes to fetch
     * @param timeout       timeout
     * @param retryTimes    retry times
     * @param dataNodeIndex data node index. A negative value means to fetch from the primary node.
     * @return record batches
     */
    private CompletableFuture<List<RecordBatch>> fetchBatches0(long streamId, long startOffset, int minBytes,
        int maxBytes, Duration timeout, int retryTimes, int dataNodeIndex) {
        if (retryTimes <= 0) {
            CompletableFuture<List<RecordBatch>> future = new CompletableFuture<>();
            future.completeExceptionally(new Exception("Retry times exceeded"));
            return future;
        }

        FetchInfoT fetchInfoT = new FetchInfoT();
        fetchInfoT.setRequestIndex(0);
        fetchInfoT.setFetchOffset(startOffset);
        fetchInfoT.setBatchMaxBytes(maxBytes);
        FetchRequestT fetchRequestT = new FetchRequestT();
        fetchRequestT.setMaxWaitMs((int) timeout.toMillis());
        fetchRequestT.setMinBytes(minBytes);
        fetchRequestT.setFetchRequests(new FetchInfoT[] {fetchInfoT});

        return this.streamRangeCache.getFloorRange(streamId, startOffset).thenCompose(rangeT -> {
            fetchInfoT.setRange(rangeT);
            FlatBufferBuilder builder = new FlatBufferBuilder();
            int fetchRequestOffset = FetchRequest.pack(builder, fetchRequestT);
            builder.finish(fetchRequestOffset);
            SbpFrame sbpFrame = ProtocolUtil.constructRequestSbpFrame(OperationCode.FETCH, builder.dataBuffer());

            RangeTDecorator rangeTDecorator = new RangeTDecorator(rangeT);

            // If dataNode is not specified, fetch from the primary node.
            int dnIndex = dataNodeIndex >= 0 ? dataNodeIndex : rangeTDecorator.getPrimaryDnIndex();
            Address dataNodeAddress = Address.fromAddress(rangeT.getReplicaNodes()[dnIndex].getDataNode().getAdvertiseAddr());
            log.debug("trying to fetch batches from datanode {}, streamId {}, rangeIndex {}, startOffset {}", dataNodeAddress, rangeT.getStreamId(), rangeT.getRangeIndex(), startOffset);

            return nettyClient.invokeAsync(dataNodeAddress, sbpFrame, timeout)
                .thenCompose(responseFrame -> {
                    FetchResponse response = FetchResponse.getRootAsFetchResponse(responseFrame.getHeader());
                    return extractResponse(response).thenCompose(list -> {
                        CompletableFuture<List<RecordBatch>> future = new CompletableFuture<>();
                        short code = list.get(0).getStatus().getCode();
                        // Get valid batches.
                        if (code == OK) {
                            int count = list.get(0).getBatchCount();
                            future.complete(RecordBatch.decode(responseFrame.getPayload()[0], count));
                            return future;
                        }

                        // return empty List if no new record.
                        if (code == NO_NEW_RECORD) {
                            future.complete(Collections.emptyList());
                            return future;
                        }

                        log.warn("Fetch batches from datanode {} failed, error code {}, msg {} ", dataNodeAddress, code, list.get(0).getStatus().getMessage());

                        // No need to retry.
                        if (code == OFFSET_OVERFLOW) {
                            future.completeExceptionally(new ClientException("Fetch batches failed with code " + code + ", msg: " + list.get(0).getStatus().getMessage()));
                            return future;
                        }

                        // invalid the cache and retry.
                        if (code == RANGE_NOT_FOUND || code == OFFSET_OUT_OF_RANGE_BOUNDS) {
                            this.streamRangeCache.invalidate(streamId);
                            return fetchBatches0(streamId, startOffset, minBytes, maxBytes, timeout, retryTimes - 1, -1);
                        }

                        // Try next node again if possible.
                        return fetchBatches0(streamId, startOffset, minBytes, maxBytes, timeout, retryTimes - 1, rangeTDecorator.getNextDnIndex(dnIndex));
                    });
                });
        });
    }

    private CompletableFuture<List<AppendResultT>> extractResponse(AppendResponse response) {
        CompletableFuture<List<AppendResultT>> future = new CompletableFuture<>();
        if (response.status().code() != OK) {
            future.completeExceptionally(new ClientException("Append batch failed with code " + response.status().code() + ", msg: " + response.status().message()));
            return future;
        }
        List<AppendResultT> appendResultList = Arrays.asList(response.unpack().getAppendResponses());

        // invalidate the cache if the append request is successful.
        appendResultList.forEach(appendResultT -> {
            if (appendResultT.getStatus().getCode() == OK) {
                this.streamRangeCache.invalidate(appendResultT.getStreamId());
            }
        });
        future.complete(appendResultList);
        return future;
    }

    /**
     * Update the cache based on the received seal ranges result list.
     *
     * @param sealResultList the seal ranges result list.
     * @return the Void completable future.
     */
    private CompletableFuture<Void> handleSealRangesResultList(List<SealRangesResultT> sealResultList) {
        return CompletableFuture.allOf(sealResultList.stream().map(sealResultT -> {
            if (sealResultT.getStatus().getCode() != OK) {
                return CompletableFuture.completedFuture(null);
            }

            long streamId = sealResultT.getRange().getStreamId();
            // update the ranges.
            return streamRangeCache.getLastRange(streamId)
                .thenAccept(rangeT -> {
                    // If the same last range is returned, update the last range.
                    if (rangeT.getRangeIndex() == sealResultT.getRange().getRangeIndex()) {
                        streamRangeCache.get(streamId).put(sealResultT.getRange().getStartOffset(), sealResultT.getRange());
                        return;
                    }
                    // If the next range is returned, update the last range and put the new range.
                    if ((rangeT.getRangeIndex() + 1) == sealResultT.getRange().getRangeIndex()) {
                        streamRangeCache.get(streamId).put(rangeT.getStartOffset(), rangeT);
                        streamRangeCache.get(streamId).put(sealResultT.getRange().getStartOffset(), sealResultT.getRange());
                        return;
                    }
                    // If the next range is not returned, invalidate the cache.
                    streamRangeCache.invalidate(streamId);
                });
        }).toArray(CompletableFuture[]::new));
    }

    private CompletableFuture<Boolean> extractResponse(HeartbeatResponse response) {
        CompletableFuture<Boolean> future = new CompletableFuture<>();
        if (response.status().code() != OK) {
            future.completeExceptionally(new ClientException("Heartbeat failed with code " + response.status().code() + ", msg: " + response.status().message() + ", clientId: " + response.clientId()));
            return future;
        }
        log.debug("Heartbeat success, clientId: {}", response.clientId());
        future.complete(true);
        return future;
    }

    private CompletableFuture<List<FetchResultT>> extractResponse(FetchResponse response) {
        CompletableFuture<List<FetchResultT>> future = new CompletableFuture<>();
        if (response.status().code() != OK) {
            future.completeExceptionally(new ClientException("Fetch batches failed with code " + response.status().code() + ", msg: " + response.status().message()));
            return future;
        }
        future.complete(Arrays.asList(response.unpack().getFetchResponses()));
        return future;
    }

    private CompletableFuture<RangeT[]> fetchRangeArrayBasedOnStreamId(long streamId) {
        RangeCriteriaT rangeCriteriaT = new RangeCriteriaT();
        // no need to setDataNode since we only need to get the ranges based on streamId.
        rangeCriteriaT.setStreamId(streamId);
        return resourceManager.listRanges(Collections.singletonList(rangeCriteriaT), CACHE_LOAD_TIMEOUT)
            .thenApply(list -> list.get(0).getRanges());
    }

    /**
     * Keep the netty channels alive by sending heartbeat request to the server.
     */
    private void keepNettyChannelsAlive() {
        for (Map.Entry<Channel, Boolean> item : this.nettyClient.aliveChannelTable.entrySet()) {
            if (item.getValue()) {
                Channel channel = item.getKey();
                Address address = RemotingUtil.parseChannelRemoteAddress(channel);
                try {
                    this.heartbeat(address, Duration.ofSeconds(3)).join();
                    log.debug("Successfully refresh channel with address {} ", address);
                } catch (Throwable ex) {
                    log.error("Failed to refresh channel with address {} ", address, ex);
                }
            }
        }
    }

    /**
     * Generate SbpFrames for Append based on provided recordBatches.
     * Note that recordBatches with the same streamId are grouped into the same SbpFrame.
     *
     * @param recordBatches recordBatches to be sent to server. They may contain different streamId.
     * @param timeoutMillis timeout for each AppendRequest.
     * @return Map of streamId to SbpFrame.
     */
    private Map<Long, SbpFrame> generateAppendRequest(List<RecordBatch> recordBatches, int timeoutMillis) {
        // streamId -> List<AppendInfoT>
        Map<Long, List<AppendInfoT>> appendInfoTMap = new HashMap<>();
        // streamId -> request_index
        Map<Long, Integer> appendInfoIndexMap = new HashMap<>();
        // streamId -> payloadList
        Map<Long, List<ByteBuffer>> payloadMap = new HashMap<>();

        for (RecordBatch batch : recordBatches) {
            // no need to send empty batch
            if (batch.getRecords() == null || batch.getRecords().size() == 0) {
                continue;
            }
            Long streamId = batch.getBatchMeta().getStreamId();

            AppendInfoT appendInfoT = new AppendInfoT();
            ByteBuffer encodedBuffer = batch.encode();
            this.streamRangeCache.getLastRange(streamId).thenAccept(appendInfoT::setRange).join();
            appendInfoT.setBatchLength(encodedBuffer.remaining());

            // find the request index in the appendRequest for this batch
            int index = appendInfoIndexMap.getOrDefault(streamId, 0);
            appendInfoT.setRequestIndex(index);
            appendInfoIndexMap.put(streamId, index + 1);

            // add to the right batch list
            appendInfoTMap.computeIfAbsent(streamId, key -> new ArrayList<>())
                .add(appendInfoT);
            payloadMap.computeIfAbsent(streamId, key -> new ArrayList<>())
                .add(encodedBuffer);
        }

        Map<Long, SbpFrame> streamIdToSbpFrameMap = new HashMap<>(appendInfoTMap.size());
        appendInfoTMap.forEach((streamId, appendInfoTList) -> {
            AppendRequestT appendRequestT = new AppendRequestT();
            appendRequestT.setTimeoutMs(timeoutMillis);
            appendRequestT.setAppendRequests(appendInfoTList.toArray(new AppendInfoT[0]));

            FlatBufferBuilder builder = new FlatBufferBuilder();
            int pack = AppendRequest.pack(builder, appendRequestT);
            builder.finish(pack);

            SbpFrame sbpFrame = ProtocolUtil.constructRequestSbpFrame(OperationCode.APPEND, builder.dataBuffer(), payloadMap.get(streamId).toArray(new ByteBuffer[0]));
            streamIdToSbpFrameMap.put(streamId, sbpFrame);
        });

        return streamIdToSbpFrameMap;
    }

    @Override
    public void close() throws IOException {
        this.timer.stop();
        this.nettyClient.close();
    }
}
