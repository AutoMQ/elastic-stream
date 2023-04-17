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
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import sdk.elastic.stream.apis.ClientConfiguration;
import sdk.elastic.stream.apis.OperationClient;
import sdk.elastic.stream.apis.exception.ClientException;
import sdk.elastic.stream.apis.exception.OutdatedCacheException;
import sdk.elastic.stream.apis.exception.RetryableException;
import sdk.elastic.stream.apis.exception.SealableException;
import sdk.elastic.stream.apis.manager.ResourceManager;
import sdk.elastic.stream.client.common.ClientId;
import sdk.elastic.stream.client.common.ProtocolUtil;
import sdk.elastic.stream.client.common.RemotingUtil;
import sdk.elastic.stream.client.netty.NettyClient;
import sdk.elastic.stream.client.protocol.SbpFrame;
import sdk.elastic.stream.client.protocol.StatusTDecorator;
import sdk.elastic.stream.client.route.Address;
import sdk.elastic.stream.flatc.header.AppendInfoT;
import sdk.elastic.stream.flatc.header.AppendRequest;
import sdk.elastic.stream.flatc.header.AppendRequestT;
import sdk.elastic.stream.flatc.header.AppendResponse;
import sdk.elastic.stream.flatc.header.AppendResultT;
import sdk.elastic.stream.flatc.header.ClientRole;
import sdk.elastic.stream.flatc.header.CreateStreamResultT;
import sdk.elastic.stream.flatc.header.DataNodeT;
import sdk.elastic.stream.flatc.header.DescribeRangeResultT;
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
import sdk.elastic.stream.models.RecordBatchReader;

import static sdk.elastic.stream.flatc.header.ErrorCode.DN_INTERNAL_SERVER_ERROR;
import static sdk.elastic.stream.flatc.header.ErrorCode.NO_NEW_RECORD;
import static sdk.elastic.stream.flatc.header.ErrorCode.OFFSET_OUT_OF_RANGE_BOUNDS;
import static sdk.elastic.stream.flatc.header.ErrorCode.OFFSET_OVERFLOW;
import static sdk.elastic.stream.flatc.header.ErrorCode.OK;
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
        this.streamRangeCache = new StreamRangeCache(clientConfiguration.getStreamCacheSize(),
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

        return appendBatch0(recordBatch, timeout).exceptionally(throwable -> {
            if (throwable instanceof OutdatedCacheException) {
                this.streamRangeCache.invalidate(streamId);
                return appendBatch0(recordBatch, timeout).join();
            }
            if (throwable instanceof SealableException) {
                log.error("Got server error when appending a batch for streamId {}. Try to seal it", streamId);
                RangeIdT rangeIdT = new RangeIdT();
                rangeIdT.setStreamId(streamId);
                rangeIdT.setRangeIndex(0);
                // seal the range and try again.
                return resourceManager.sealRanges(Collections.singletonList(rangeIdT), timeout)
                    .thenCompose(sealResultList -> {
                        log.info("sealed for streamId {}, result: {}", streamId, sealResultList.get(0));
                        return handleSealRangesResultList(sealResultList).thenCompose(v -> appendBatch0(recordBatch, timeout));
                    }).join();
            }
            log.error("Failed to append batch for streamId {}. Error: {}", recordBatch.getBatchMeta().getStreamId(), throwable.getMessage());
            return null;
        });
    }

    @Override
    public CompletableFuture<Long> getLastWritableOffset(long streamId, Duration timeout) {
        return getLastWritableOffset0(streamId, timeout).thenCompose(rangeT -> {
            // The requested range has been sealed, so we have to invalidate the cache and try again.
            if (rangeT.getEndOffset() > 0) {
                this.streamRangeCache.invalidate(streamId);
                return getLastWritableOffset0(streamId, timeout);
            }
            return CompletableFuture.completedFuture(rangeT);
        }).thenApply(RangeT::getNextOffset);
    }

    @Override
    public CompletableFuture<List<RecordBatch>> fetchBatches(long streamId, long startOffset, int minBytes,
        int maxBytes, Duration timeout) {
        // Batches will be fetched from the primary node. If the primary node is unavailable, the next node will be accessed.
        return fetchBatches0(streamId, startOffset, minBytes, maxBytes, timeout, true)
            .exceptionally(ex -> {
                // invalid the cache and retry.
                if (ex instanceof OutdatedCacheException) {
                    this.streamRangeCache.invalidate(streamId);
                    return fetchBatches0(streamId, startOffset, minBytes, maxBytes, timeout, true).join();
                }
                // just retry.
                if (ex instanceof RetryableException) {
                    return fetchBatches0(streamId, startOffset, minBytes, maxBytes, timeout, false).join();
                }

                // In other cases, do not retry and return null;
                log.error("Failed to fetch batches for streamId {} from offset {}. minBytes {}, maxBytes {}, timeout {}", streamId, startOffset, minBytes, maxBytes, timeout, ex);
                return null;
            });
    }

    @Override
    public CompletableFuture<Boolean> heartbeat(Address address, Duration timeout) {
        return heartbeat0(address, timeout).exceptionally(ex -> {
            if (ex instanceof RetryableException) {
                return heartbeat0(address, timeout).join();
            }
            return false;
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

    private CompletableFuture<Boolean> heartbeat0(Address address, Duration timeout) {
        HeartbeatRequestT heartbeatRequestT = new HeartbeatRequestT();
        heartbeatRequestT.setClientId(getClientId().toString());
        heartbeatRequestT.setClientRole(ClientRole.CLIENT_ROLE_CUSTOMER);
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int heartbeatRequestOffset = HeartbeatRequest.pack(builder, heartbeatRequestT);
        builder.finish(heartbeatRequestOffset);

        SbpFrame sbpFrame = ProtocolUtil.constructRequestSbpFrame(OperationCode.HEARTBEAT, builder.dataBuffer());
        return nettyClient.invokeAsync(address, sbpFrame, timeout)
            .thenCompose(responseFrame -> {
                CompletableFuture<Boolean> future = new CompletableFuture<>();

                HeartbeatResponse response = HeartbeatResponse.getRootAsHeartbeatResponse(responseFrame.getHeader());
                Address updatePmAddress = new StatusTDecorator(response.status()).maybeGetNewPmAddress();
                // need to connect to new Pm primary node.
                if (updatePmAddress != null) {
                    nettyClient.updatePmAddress(updatePmAddress);
                    future.completeExceptionally(new RetryableException("Pm address changed"));
                    return future;
                }

                if (response.status().code() != OK) {
                    future.completeExceptionally(new ClientException("Heartbeat failed with code " + response.status().code() + ", msg: " + response.status().message() + ", clientId: " + response.clientId()));
                    return future;
                }
                log.debug("Heartbeat success, clientId: {}", response.clientId());
                future.complete(true);
                return future;
            });
    }

    private CompletableFuture<AppendResultT> appendBatch0(RecordBatch recordBatch, Duration timeout) {
        long streamId = recordBatch.getBatchMeta().getStreamId();

        AtomicReference<Address> dataNodeAddress = new AtomicReference<>();
        CompletableFuture<SbpFrame> frameFuture = this.streamRangeCache.getLastRange(streamId).thenApply(rangeT -> {
            RangeTDecorator rangeTDecorator = new RangeTDecorator(rangeT);
            dataNodeAddress.set(Address.fromAddress(rangeTDecorator.getPrimaryNode().getAdvertiseAddr()));

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

            return ProtocolUtil.constructRequestSbpFrame(OperationCode.APPEND, builder.dataBuffer(), new ByteBuffer[] {encodedBuffer});
        });

        return frameFuture.thenCompose(sbpFrame -> {
            log.debug("trying to append a batch for streamId {} to datanode {}", streamId, dataNodeAddress.get().getAddress());
            return nettyClient.invokeAsync(dataNodeAddress.get(), sbpFrame, timeout);
        }).thenCompose(responseFrame -> {
            AppendResponse response = AppendResponse.getRootAsAppendResponse(responseFrame.getHeader());

            CompletableFuture<AppendResultT> future = new CompletableFuture<>();
            short errorCode = response.status().code();
            if (errorCode != OK) {
                future.completeExceptionally(new ClientException("Wrapped response of Append batches failed with code " + errorCode + ", msg: " + response.status().message()));
                return future;
            }
            future.complete(response.unpack().getAppendResponses()[0]);
            return future;
        }).thenCompose(appendResult -> {
            CompletableFuture<AppendResultT> future = new CompletableFuture<>();
            short errorCode = appendResult.getStatus().getCode();

            if (errorCode == OK) {
                future.complete(appendResult);
                return future;
            }

            String thrownMsg = "Append batch failed with code " + errorCode + ", msg: " + appendResult.getStatus().getMessage();
            // cache needs to be updated.
            if (errorCode == RANGE_ALREADY_SEALED) {
                future.completeExceptionally(new OutdatedCacheException(thrownMsg));
                return future;
            }
            // For errors that are related to the data nodes, seal the range.
            if (errorCode >= DN_INTERNAL_SERVER_ERROR) {
                future.completeExceptionally(new SealableException(thrownMsg));
                return future;
            }
            // for other errors, just return the error.
            future.completeExceptionally(new ClientException(thrownMsg));
            return future;
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
            return this.resourceManager.describeRanges(dataNodeAddress, Collections.singletonList(rangeIdT), timeout);
        }).thenCompose(list -> {
            CompletableFuture<RangeT> future = new CompletableFuture<>();
            DescribeRangeResultT describeRangeResultT = list.get(0);
            if (describeRangeResultT.getStatus().getCode() != OK) {
                future.completeExceptionally(new ClientException("Get last writableOffset failed with code " + describeRangeResultT.getStatus().getCode() + ", msg: " + describeRangeResultT.getStatus().getMessage()));
                return future;
            }
            future.complete(describeRangeResultT.getRange());
            return future;
        });
    }

    /**
     * Fetch batches from data nodes with retry.
     *
     * @param streamId       stream id
     * @param startOffset    start offset
     * @param minBytes       min bytes to fetch
     * @param maxBytes       max bytes to fetch
     * @param timeout        timeout
     * @param usePrimaryNode whether to use primary node
     * @return record batches
     */
    private CompletableFuture<List<RecordBatch>> fetchBatches0(long streamId, long startOffset, int minBytes,
        int maxBytes, Duration timeout, boolean usePrimaryNode) {
        FetchInfoT fetchInfoT = new FetchInfoT();
        fetchInfoT.setRequestIndex(0);
        fetchInfoT.setFetchOffset(startOffset);
        fetchInfoT.setBatchMaxBytes(maxBytes);
        FetchRequestT fetchRequestT = new FetchRequestT();
        fetchRequestT.setMaxWaitMs((int) timeout.toMillis());
        fetchRequestT.setMinBytes(minBytes);
        fetchRequestT.setFetchRequests(new FetchInfoT[] {fetchInfoT});

        AtomicReference<Address> dataNodeAddress = new AtomicReference<>();

        CompletableFuture<SbpFrame> frameFuture = this.streamRangeCache.getFloorRange(streamId, startOffset)
            .thenApply(rangeT -> {
                fetchInfoT.setRange(rangeT);
                FlatBufferBuilder builder = new FlatBufferBuilder();
                int fetchRequestOffset = FetchRequest.pack(builder, fetchRequestT);
                builder.finish(fetchRequestOffset);

                RangeTDecorator rangeTDecorator = new RangeTDecorator(rangeT);
                DataNodeT dataNodeT = usePrimaryNode ? rangeTDecorator.getPrimaryNode() : rangeTDecorator.maybeGetNonPrimaryNode();
                dataNodeAddress.set(Address.fromAddress(dataNodeT.getAdvertiseAddr()));

                return ProtocolUtil.constructRequestSbpFrame(OperationCode.FETCH, builder.dataBuffer());
            });

        CompletableFuture<RecordBatchReader> fetchResultFuture = frameFuture
            .thenCompose(sbpFrame -> nettyClient.invokeAsync(dataNodeAddress.get(), sbpFrame, timeout))
            .thenCompose(responseFrame -> {
                FetchResponse response = FetchResponse.getRootAsFetchResponse(responseFrame.getHeader());
                CompletableFuture<RecordBatchReader> future = new CompletableFuture<>();
                if (response.status().code() != OK) {
                    future.completeExceptionally(new ClientException("Fetch batches failed with code " + response.status().code() + ", msg: " + response.status().message()));
                    return future;
                }

                FetchResultT fetchResultT = response.unpack().getFetchResponses()[0];
                future.complete(new RecordBatchReader(fetchResultT.getStatus(), responseFrame.getPayload(), fetchResultT.getBatchCount()));
                return future;
            });

        return fetchResultFuture.thenCompose(batchReader -> {
            CompletableFuture<List<RecordBatch>> future = new CompletableFuture<>();
            short code = batchReader.getStatusT().getCode();
            // Get valid batches.
            if (code == OK) {
                future.complete(batchReader.getRecordBatches());
                return future;
            }

            // return empty List if no new record.
            if (code == NO_NEW_RECORD) {
                future.complete(Collections.emptyList());
                return future;
            }

            log.warn("Fetch batches from datanode {} failed, error code {}, msg {} ", dataNodeAddress.get(), code, batchReader.getStatusT().getMessage());

            String throwMessage = "Fetch batches from datanode " + dataNodeAddress.get() + " failed, error code " + code + ", msg " + batchReader.getStatusT().getMessage();
            if (code == OFFSET_OVERFLOW) {
                future.completeExceptionally(new ClientException(throwMessage));
            } else if (code == RANGE_NOT_FOUND || code == OFFSET_OUT_OF_RANGE_BOUNDS) {
                future.completeExceptionally(new OutdatedCacheException(throwMessage));
            } else {
                future.completeExceptionally(new RetryableException(throwMessage));
            }

            return future;
        });
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
