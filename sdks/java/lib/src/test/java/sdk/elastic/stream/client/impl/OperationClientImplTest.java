package sdk.elastic.stream.client.impl;

import com.google.flatbuffers.FlatBufferBuilder;
import java.nio.ByteBuffer;
import java.time.Duration;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutionException;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.mockito.Mock;
import sdk.elastic.stream.apis.OperationClient;
import sdk.elastic.stream.apis.manager.ResourceManager;
import sdk.elastic.stream.client.common.ClientId;
import sdk.elastic.stream.client.netty.NettyClient;
import sdk.elastic.stream.client.protocol.SbpFrame;
import sdk.elastic.stream.client.route.Address;
import sdk.elastic.stream.flatc.header.AppendResponseT;
import sdk.elastic.stream.flatc.header.AppendResultT;
import sdk.elastic.stream.flatc.header.ClientRole;
import sdk.elastic.stream.flatc.header.CreateStreamResultT;
import sdk.elastic.stream.flatc.header.DataNodeT;
import sdk.elastic.stream.flatc.header.DescribeRangeResultT;
import sdk.elastic.stream.flatc.header.ErrorCode;
import sdk.elastic.stream.flatc.header.FetchResponseT;
import sdk.elastic.stream.flatc.header.FetchResultT;
import sdk.elastic.stream.flatc.header.HeartbeatResponse;
import sdk.elastic.stream.flatc.header.HeartbeatResponseT;
import sdk.elastic.stream.flatc.header.RangeT;
import sdk.elastic.stream.flatc.header.ReplicaNodeT;
import sdk.elastic.stream.flatc.header.StatusT;
import sdk.elastic.stream.flatc.header.StreamT;
import sdk.elastic.stream.models.OperationCode;
import sdk.elastic.stream.models.RecordBatch;
import sdk.elastic.stream.models.RecordBatchesGenerator;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.doReturn;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.verify;

class OperationClientImplTest {

    @Mock
    private NettyClient nettyClient;
    @Mock
    private ResourceManager resourceManager;
    @Mock
    private StreamRangeCache streamRangeCache;
    private OperationClient operationClient;
    private static final Address defaultPmAddress = new Address("localhost", 2080);
    private static final Duration DEFAULT_REQUEST_TIMEOUT = Duration.ofSeconds(3);
    private static StatusT okStatus;
    private static ReplicaNodeT replicaNodeT;

    @BeforeAll
    static void beforeAll() {
        okStatus = new StatusT();
        okStatus.setCode(ErrorCode.OK);
        okStatus.setMessage("OK");

        DataNodeT dataNodeT = new DataNodeT();
        dataNodeT.setNodeId(1);
        dataNodeT.setAdvertiseAddr("localhost:8080");

        replicaNodeT = new ReplicaNodeT();
        replicaNodeT.setDataNode(dataNodeT);
        replicaNodeT.setIsPrimary(true);
    }

    @BeforeEach
    void setUp() {
        nettyClient = mock(NettyClient.class);
        resourceManager = mock(ResourceManager.class);
        streamRangeCache = mock(StreamRangeCache.class);
        operationClient = new OperationClientImpl(Duration.ofSeconds(10), nettyClient, resourceManager, streamRangeCache);
    }

    @Test
    void createStreams() throws ExecutionException, InterruptedException {
        long streamId = 0;
        byte replicaNum = 1;
        long retentionPeriodMs = Duration.ofDays(21).toMillis();

        RangeT rangeT = new RangeT();
        rangeT.setStreamId(streamId);
        rangeT.setReplicaNodes(new ReplicaNodeT[] {replicaNodeT});
        rangeT.setRangeIndex(0);
        rangeT.setStartOffset(0);
        rangeT.setEndOffset(-1);
        rangeT.setNextOffset(0);
        StreamT streamT = new StreamT();
        streamT.setStreamId(streamId);
        streamT.setReplicaNums(replicaNum);
        streamT.setRetentionPeriodMs(retentionPeriodMs);

        CreateStreamResultT createStreamResultT = new CreateStreamResultT();
        createStreamResultT.setStatus(okStatus);
        createStreamResultT.setRange(rangeT);
        createStreamResultT.setStream(streamT);
        doReturn(CompletableFuture.completedFuture(Collections.singletonList(createStreamResultT))).when(resourceManager).createStreams(any(), any(Duration.class));

        StreamT toCreatestreamT = new StreamT();
        toCreatestreamT.setReplicaNums(replicaNum);
        toCreatestreamT.setRetentionPeriodMs(retentionPeriodMs);
        operationClient.createStreams(Collections.singletonList(toCreatestreamT), DEFAULT_REQUEST_TIMEOUT).thenAccept(
            resultTList -> {
                assertEquals(streamId, resultTList.get(0).getStream().getStreamId());
                verify(streamRangeCache).put(streamId, new RangeT[] {rangeT});
            }
        ).get();

    }

    @Test
    void appendBatch() throws ExecutionException, InterruptedException {
        long streamId = 1;
        long batchAppendBaseOffset = 110;
        long lastRangeBaseOffset = batchAppendBaseOffset - 10;
        long appendTimestamp = System.currentTimeMillis();

        RangeT rangeT = new RangeT();
        rangeT.setStreamId(streamId);
        rangeT.setReplicaNodes(new ReplicaNodeT[] {replicaNodeT});
        rangeT.setRangeIndex(1);
        rangeT.setStartOffset(lastRangeBaseOffset);
        rangeT.setEndOffset(-1);
        rangeT.setNextOffset(batchAppendBaseOffset);
        doReturn(CompletableFuture.completedFuture(rangeT)).when(streamRangeCache).getLastRange(streamId);

        AppendResultT appendResultT = new AppendResultT();
        appendResultT.setStreamId(streamId);
        appendResultT.setRequestIndex(0);
        appendResultT.setBaseOffset(batchAppendBaseOffset);
        appendResultT.setStreamAppendTimeMs(appendTimestamp);
        appendResultT.setStatus(okStatus);

        AppendResponseT appendResponseT = new AppendResponseT();
        appendResponseT.setStatus(okStatus);
        appendResponseT.setThrottleTimeMs(0);
        appendResponseT.setAppendResponses(new AppendResultT[] {appendResultT});
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int appendResponseOffset = sdk.elastic.stream.flatc.header.AppendResponse.pack(builder, appendResponseT);
        builder.finish(appendResponseOffset);

        SbpFrame mockResponseFrame = SbpFrame.newBuilder()
            .setFlag((byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG))
            .setOperationCode(OperationCode.APPEND.getCode())
            .setHeader(builder.dataBuffer())
            .build();
        doReturn(CompletableFuture.completedFuture(mockResponseFrame)).when(nettyClient).invokeAsync(any(Address.class), any(SbpFrame.class), any(Duration.class));

        RecordBatch batch = RecordBatchesGenerator.generateOneRecordBatch(streamId);
        AppendResultT resultT = operationClient.appendBatch(batch, DEFAULT_REQUEST_TIMEOUT).get();
        assertEquals(streamId, resultT.getStreamId());
        assertEquals(0, resultT.getRequestIndex());
        assertTrue(resultT.getBaseOffset() >= lastRangeBaseOffset);
        assertEquals(batchAppendBaseOffset, resultT.getBaseOffset());
        assertEquals(appendTimestamp, resultT.getStreamAppendTimeMs());
        assertEquals(ErrorCode.OK, resultT.getStatus().getCode());
    }

    @Test
    void getLastWritableOffset() throws ExecutionException, InterruptedException {
        long streamId = 2;
        long baseOffset = 110;
        long nextOffset = baseOffset + 15;
        long realNextOffset = nextOffset + 20;

        RangeT rangeT = new RangeT();
        rangeT.setStreamId(streamId);
        rangeT.setReplicaNodes(new ReplicaNodeT[] {replicaNodeT});
        rangeT.setRangeIndex(1);
        rangeT.setStartOffset(baseOffset);
        rangeT.setEndOffset(-1);
        rangeT.setNextOffset(nextOffset);
        doReturn(CompletableFuture.completedFuture(rangeT)).when(streamRangeCache).getLastRange(streamId);

        RangeT realRangeT = new RangeT();
        realRangeT.setStreamId(rangeT.getStreamId());
        realRangeT.setReplicaNodes(rangeT.getReplicaNodes());
        realRangeT.setRangeIndex(rangeT.getRangeIndex());
        realRangeT.setStartOffset(rangeT.getStartOffset());
        realRangeT.setEndOffset(rangeT.getEndOffset());
        realRangeT.setNextOffset(realNextOffset);
        DescribeRangeResultT describeRangeResultT = new DescribeRangeResultT();
        describeRangeResultT.setStatus(okStatus);
        describeRangeResultT.setStreamId(streamId);
        describeRangeResultT.setRange(realRangeT);
        doReturn(CompletableFuture.completedFuture(Collections.singletonList(describeRangeResultT))).when(resourceManager).describeRanges(any(Address.class), any(), any(Duration.class));

        Long aLong = operationClient.getLastWritableOffset(streamId, DEFAULT_REQUEST_TIMEOUT).get();
        assertTrue(aLong >= nextOffset);
    }

    @Test
    void fetchBatches() throws ExecutionException, InterruptedException {
        long streamId = 3;
        long startOffset = 89;
        long baseOffset = 100;
        long endOffset = baseOffset + 50;
        int minBytes = 10;
        int maxBytes = Integer.MAX_VALUE - 1;

        RecordBatch batch = RecordBatchesGenerator.generateOneRecordBatch(streamId, baseOffset);
        ByteBuffer encodedBuffer = batch.encode();

        RangeT rangeT = new RangeT();
        rangeT.setStreamId(streamId);
        rangeT.setReplicaNodes(new ReplicaNodeT[] {replicaNodeT});
        rangeT.setRangeIndex(1);
        rangeT.setStartOffset(baseOffset);
        rangeT.setEndOffset(endOffset);
        rangeT.setNextOffset(endOffset);
        doReturn(CompletableFuture.completedFuture(rangeT)).when(streamRangeCache).getFloorRange(streamId, startOffset);

        FetchResultT fetchResultT = new FetchResultT();
        fetchResultT.setStreamId(streamId);
        fetchResultT.setRequestIndex(0);
        fetchResultT.setBatchCount(batch.getRecords().size());
        fetchResultT.setStatus(okStatus);

        FetchResponseT fetchResponseT = new FetchResponseT();
        fetchResponseT.setStatus(okStatus);
        fetchResponseT.setThrottleTimeMs(0);
        fetchResponseT.setFetchResponses(new FetchResultT[] {fetchResultT});
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int fetchResponseOffset = sdk.elastic.stream.flatc.header.FetchResponse.pack(builder, fetchResponseT);
        builder.finish(fetchResponseOffset);

        SbpFrame mockResponseFrame = SbpFrame.newBuilder()
            .setFlag((byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG))
            .setOperationCode(OperationCode.FETCH.getCode())
            .setHeader(builder.dataBuffer())
            .setPayload(new ByteBuffer[] {encodedBuffer.duplicate()})
            .build();
        doReturn(CompletableFuture.completedFuture(mockResponseFrame)).when(nettyClient).invokeAsync(any(Address.class), any(SbpFrame.class), any(Duration.class));

        List<RecordBatch> batches = operationClient.fetchBatches(streamId, startOffset, minBytes, maxBytes, DEFAULT_REQUEST_TIMEOUT).get();
        Integer totalBytes = batches.stream().reduce(0, (sum, item) -> sum + item.encode().remaining(), Integer::sum);
        assertTrue(totalBytes >= minBytes);
        assertTrue(totalBytes <= maxBytes);
        assertEquals(1, batches.size());
        assertTrue(batches.get(0).getBaseOffset() >= startOffset);
        assertEquals(batch.getRecords(), batches.get(0).getRecords());
    }

    @Test
    void heartbeat() throws ExecutionException, InterruptedException {
        doReturn(new ClientId()).when(nettyClient).getClientId();

        ClientId clientId = operationClient.getClientId();
        byte clientRole = ClientRole.CLIENT_ROLE_CUSTOMER;
        HeartbeatResponseT heartbeatResponseT = new HeartbeatResponseT();
        heartbeatResponseT.setStatus(okStatus);
        heartbeatResponseT.setClientId(clientId.toString());
        heartbeatResponseT.setClientRole(clientRole);
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int heartbeatResponseOffset = HeartbeatResponse.pack(builder, heartbeatResponseT);
        builder.finish(heartbeatResponseOffset);

        SbpFrame mockResponseFrame = SbpFrame.newBuilder()
            .setFlag((byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG))
            .setOperationCode(OperationCode.HEARTBEAT.getCode())
            .setHeader(builder.dataBuffer())
            .build();

        doReturn(CompletableFuture.completedFuture(mockResponseFrame)).when(nettyClient).invokeAsync(any(Address.class), any(SbpFrame.class), any(Duration.class));
        Boolean result = operationClient.heartbeat(defaultPmAddress, DEFAULT_REQUEST_TIMEOUT).get();
        assertTrue(result);
    }

    @Test
    void getClientId() {
        doReturn(new ClientId()).when(nettyClient).getClientId();

        ClientId clientId = new ClientId();
        assertTrue(clientId.getIndex() > operationClient.getClientId().getIndex());
    }
}