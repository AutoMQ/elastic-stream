package sdk.elastic.stream.client.impl;

import com.google.flatbuffers.FlatBufferBuilder;
import java.time.Duration;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutionException;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.mockito.Mock;
import sdk.elastic.stream.apis.ClientConfiguration;
import sdk.elastic.stream.apis.manager.ResourceManager;
import sdk.elastic.stream.client.netty.NettyClient;
import sdk.elastic.stream.client.protocol.SbpFrame;
import sdk.elastic.stream.client.route.Address;
import sdk.elastic.stream.flatc.header.CreateStreamResultT;
import sdk.elastic.stream.flatc.header.CreateStreamsResponse;
import sdk.elastic.stream.flatc.header.CreateStreamsResponseT;
import sdk.elastic.stream.flatc.header.DataNodeT;
import sdk.elastic.stream.flatc.header.DescribeRangeResultT;
import sdk.elastic.stream.flatc.header.DescribeRangesResponse;
import sdk.elastic.stream.flatc.header.DescribeRangesResponseT;
import sdk.elastic.stream.flatc.header.DescribeStreamResultT;
import sdk.elastic.stream.flatc.header.DescribeStreamsResponse;
import sdk.elastic.stream.flatc.header.DescribeStreamsResponseT;
import sdk.elastic.stream.flatc.header.ErrorCode;
import sdk.elastic.stream.flatc.header.ListRangesResponse;
import sdk.elastic.stream.flatc.header.ListRangesResponseT;
import sdk.elastic.stream.flatc.header.ListRangesResultT;
import sdk.elastic.stream.flatc.header.RangeCriteriaT;
import sdk.elastic.stream.flatc.header.RangeIdT;
import sdk.elastic.stream.flatc.header.RangeT;
import sdk.elastic.stream.flatc.header.ReplicaNodeT;
import sdk.elastic.stream.flatc.header.SealRangesResponse;
import sdk.elastic.stream.flatc.header.SealRangesResponseT;
import sdk.elastic.stream.flatc.header.SealRangesResultT;
import sdk.elastic.stream.flatc.header.StatusT;
import sdk.elastic.stream.flatc.header.StreamT;
import sdk.elastic.stream.models.OperationCode;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.doReturn;
import static org.mockito.Mockito.mock;

class ResourceManagerImplTest {
    @Mock
    private NettyClient nettyClient;

    private ResourceManager resourceManager;

    private static final ClientConfiguration clientConfiguration = ClientConfiguration.newBuilder()
        .setChannelMaxIdleTime(Duration.ofSeconds(10))
        .setPmEndpoint("localhost:8080")
        .setConnectionTimeout(Duration.ofSeconds(5))
        .setHeartBeatInterval(Duration.ofSeconds(10))
        .setClientAsyncSemaphoreValue(100)
        .build();
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
        resourceManager = new ResourceManagerImpl(nettyClient);
    }

    @Test
    void listRanges() throws ExecutionException, InterruptedException {
        long streamId = 1;

        RangeT[] rangeTArray = new RangeT[2];
        rangeTArray[0] = new RangeT();
        rangeTArray[0].setStreamId(streamId);
        rangeTArray[0].setReplicaNodes(new ReplicaNodeT[] {replicaNodeT});
        rangeTArray[0].setRangeIndex(0);
        rangeTArray[0].setStartOffset(0);
        rangeTArray[0].setEndOffset(100);
        rangeTArray[0].setNextOffset(100);
        rangeTArray[1] = new RangeT();
        rangeTArray[1].setStreamId(streamId);
        rangeTArray[1].setReplicaNodes(new ReplicaNodeT[] {replicaNodeT});
        rangeTArray[1].setRangeIndex(1);
        rangeTArray[1].setStartOffset(100);
        rangeTArray[1].setEndOffset(-1);
        rangeTArray[1].setNextOffset(100);

        ListRangesResultT[] rangeResults = new ListRangesResultT[1];
        rangeResults[0] = new ListRangesResultT();
        rangeResults[0].setRanges(rangeTArray);
        rangeResults[0].setStatus(okStatus);
        rangeResults[0].setRangeCriteria(null);

        ListRangesResponseT listRangesResponseT = new ListRangesResponseT();
        listRangesResponseT.setListResponses(rangeResults);
        listRangesResponseT.setStatus(okStatus);
        listRangesResponseT.setThrottleTimeMs(0);
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int rangeResultsOffset = ListRangesResponse.pack(builder, listRangesResponseT);
        builder.finish(rangeResultsOffset);

        SbpFrame mockResponseFrame = SbpFrame.newBuilder()
            .setFlag((byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG))
            .setOperationCode(OperationCode.LIST_RANGES.getCode())
            .setHeader(builder.dataBuffer())
            .build();

        doReturn(CompletableFuture.completedFuture(mockResponseFrame)).when(nettyClient).invokeAsync(any(SbpFrame.class), any(Duration.class));

        RangeCriteriaT rangeCriteriaT = new RangeCriteriaT();
        rangeCriteriaT.setStreamId(streamId);
        RangeT[] returnedRanges = resourceManager.listRanges(Collections.singletonList(rangeCriteriaT), DEFAULT_REQUEST_TIMEOUT)
            .thenApply(list -> list.get(0).getRanges()).get();

        assertEquals(2, returnedRanges.length);
        assertEquals(rangeTArray[0].getRangeIndex(), returnedRanges[0].getRangeIndex());
        assertEquals(rangeTArray[0].getStartOffset(), returnedRanges[0].getStartOffset());
        assertEquals(rangeTArray[0].getEndOffset(), returnedRanges[0].getEndOffset());
        assertEquals(rangeTArray[0].getNextOffset(), returnedRanges[0].getNextOffset());
        assertEquals(rangeTArray[0].getReplicaNodes().length, returnedRanges[0].getReplicaNodes().length);
        assertEquals(rangeTArray[0].getReplicaNodes()[0].getDataNode().getNodeId(), returnedRanges[0].getReplicaNodes()[0].getDataNode().getNodeId());
        assertEquals(rangeTArray[0].getReplicaNodes()[0].getDataNode().getAdvertiseAddr(), returnedRanges[0].getReplicaNodes()[0].getDataNode().getAdvertiseAddr());
        assertEquals(rangeTArray[0].getReplicaNodes()[0].getIsPrimary(), returnedRanges[0].getReplicaNodes()[0].getIsPrimary());
    }

    @Test
    void pingPong() throws ExecutionException, InterruptedException {
        byte pongFlag = (byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG);
        SbpFrame mockResponseFrame = SbpFrame.newBuilder()
            .setFlag(pongFlag)
            .setOperationCode(OperationCode.PING.getCode())
            .build();

        doReturn(CompletableFuture.completedFuture(mockResponseFrame)).when(nettyClient).invokeAsync(any(SbpFrame.class), any(Duration.class));

        Byte aByte = resourceManager.pingPong(DEFAULT_REQUEST_TIMEOUT).get();
        assertEquals(pongFlag, aByte);
    }

    @Test
    void sealRanges() {
        long streamId = 2;
        int sealedRangeIndex = 0;
        int nextRangeIndex = sealedRangeIndex + 1;

        RangeT rangeT = new RangeT();
        rangeT.setStreamId(streamId);
        rangeT.setReplicaNodes(new ReplicaNodeT[] {replicaNodeT});
        rangeT.setRangeIndex(nextRangeIndex);
        rangeT.setStartOffset(100);
        rangeT.setEndOffset(-1);
        rangeT.setNextOffset(100);

        SealRangesResultT[] sealRangesResultTS = new SealRangesResultT[1];
        sealRangesResultTS[0] = new SealRangesResultT();
        sealRangesResultTS[0].setStatus(okStatus);
        sealRangesResultTS[0].setRange(rangeT);

        SealRangesResponseT sealRangesResponseT = new SealRangesResponseT();
        sealRangesResponseT.setStatus(okStatus);
        sealRangesResponseT.setThrottleTimeMs(0);
        sealRangesResponseT.setSealResponses(sealRangesResultTS);
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int sealRangesResponseOffset = SealRangesResponse.pack(builder, sealRangesResponseT);
        builder.finish(sealRangesResponseOffset);

        SbpFrame mockResponseFrame = SbpFrame.newBuilder()
            .setFlag((byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG))
            .setOperationCode(OperationCode.SEAL_RANGES.getCode())
            .setHeader(builder.dataBuffer())
            .build();

        doReturn(CompletableFuture.completedFuture(mockResponseFrame)).when(nettyClient).invokeAsync(any(SbpFrame.class), any(Duration.class));

        RangeIdT rangeIdT = new RangeIdT();
        rangeIdT.setStreamId(streamId);
        rangeIdT.setRangeIndex(sealedRangeIndex);

        SealRangesResultT result = resourceManager.sealRanges(Collections.singletonList(rangeIdT), DEFAULT_REQUEST_TIMEOUT)
            .join()
            .get(0);
        assertEquals(okStatus.getCode(), result.getStatus().getCode());
        assertEquals(streamId, result.getRange().getStreamId());
        assertEquals(nextRangeIndex, result.getRange().getRangeIndex());
        assertEquals(rangeT.getStartOffset(), result.getRange().getStartOffset());
        assertEquals(rangeT.getEndOffset(), result.getRange().getEndOffset());
        assertEquals(rangeT.getNextOffset(), result.getRange().getNextOffset());
        assertEquals(rangeT.getReplicaNodes().length, result.getRange().getReplicaNodes().length);
        assertEquals(rangeT.getReplicaNodes()[0].getDataNode().getNodeId(), result.getRange().getReplicaNodes()[0].getDataNode().getNodeId());
        assertEquals(rangeT.getReplicaNodes()[0].getDataNode().getAdvertiseAddr(), result.getRange().getReplicaNodes()[0].getDataNode().getAdvertiseAddr());
        assertEquals(rangeT.getReplicaNodes()[0].getIsPrimary(), result.getRange().getReplicaNodes()[0].getIsPrimary());
    }

    @Test
    void describeRanges() throws ExecutionException, InterruptedException {
        long streamId = 3;
        RangeT rangeT = new RangeT();
        rangeT.setStreamId(streamId);
        rangeT.setReplicaNodes(new ReplicaNodeT[] {replicaNodeT});
        rangeT.setRangeIndex(2);
        rangeT.setStartOffset(100);
        rangeT.setEndOffset(-1);
        rangeT.setNextOffset(100);

        DescribeRangeResultT describeRangeResultT = new DescribeRangeResultT();
        describeRangeResultT.setStatus(okStatus);
        describeRangeResultT.setRange(rangeT);
        describeRangeResultT.setStreamId(streamId);

        DescribeRangesResponseT describeRangesResponseT = new DescribeRangesResponseT();
        describeRangesResponseT.setStatus(okStatus);
        describeRangesResponseT.setThrottleTimeMs(0);
        describeRangesResponseT.setDescribeResponses(new DescribeRangeResultT[] {describeRangeResultT});
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int describeRangesResponseOffset = DescribeRangesResponse.pack(builder, describeRangesResponseT);
        builder.finish(describeRangesResponseOffset);

        SbpFrame mockResponseFrame = SbpFrame.newBuilder()
            .setFlag((byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG))
            .setOperationCode(OperationCode.DESCRIBE_RANGES.getCode())
            .setHeader(builder.dataBuffer())
            .build();
        doReturn(CompletableFuture.completedFuture(mockResponseFrame)).when(nettyClient).invokeAsync(any(Address.class), any(SbpFrame.class), any(Duration.class));

        DataNodeT dataNode = rangeT.getReplicaNodes()[0].getDataNode();
        RangeIdT rangeIdT = new RangeIdT();
        rangeIdT.setRangeIndex(rangeT.getRangeIndex());
        rangeIdT.setStreamId(rangeT.getStreamId());
        List<DescribeRangeResultT> describeRangeResultTList = resourceManager.describeRanges(Address.fromAddress(dataNode.getAdvertiseAddr()), Collections.singletonList(rangeIdT), DEFAULT_REQUEST_TIMEOUT).get();

        RangeT describedRange = describeRangeResultTList.get(0).getRange();
        assertEquals(streamId, describedRange.getStreamId());
        assertEquals(rangeT.getRangeIndex(), describedRange.getRangeIndex());
        assertEquals(rangeT.getStartOffset(), describedRange.getStartOffset());
        assertEquals(rangeT.getEndOffset(), describedRange.getEndOffset());
        assertEquals(rangeT.getNextOffset(), describedRange.getNextOffset());
        assertEquals(rangeT.getReplicaNodes().length, describedRange.getReplicaNodes().length);
        assertEquals(rangeT.getReplicaNodes()[0].getDataNode().getNodeId(), describedRange.getReplicaNodes()[0].getDataNode().getNodeId());
        assertEquals(rangeT.getReplicaNodes()[0].getDataNode().getAdvertiseAddr(), describedRange.getReplicaNodes()[0].getDataNode().getAdvertiseAddr());
        assertEquals(rangeT.getReplicaNodes()[0].getIsPrimary(), describedRange.getReplicaNodes()[0].getIsPrimary());
    }

    @Test
    void describeStreams() throws ExecutionException, InterruptedException {
        long streamId = 4;
        StreamT streamT = new StreamT();
        streamT.setStreamId(streamId);
        streamT.setReplicaNums((byte) 1);
        streamT.setRetentionPeriodMs(Duration.ofDays(21).toMillis());
        DescribeStreamResultT describeStreamResultT = new DescribeStreamResultT();
        describeStreamResultT.setStatus(okStatus);
        describeStreamResultT.setStream(streamT);

        DescribeStreamsResponseT describeStreamsResponseT = new DescribeStreamsResponseT();
        describeStreamsResponseT.setStatus(okStatus);
        describeStreamsResponseT.setThrottleTimeMs(0);
        describeStreamsResponseT.setDescribeResponses(new DescribeStreamResultT[] {describeStreamResultT});
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int describeStreamsResponseOffset = DescribeStreamsResponse.pack(builder, describeStreamsResponseT);
        builder.finish(describeStreamsResponseOffset);

        SbpFrame mockResponseFrame = SbpFrame.newBuilder()
            .setFlag((byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG))
            .setOperationCode(OperationCode.DESCRIBE_STREAMS.getCode())
            .setHeader(builder.dataBuffer())
            .build();
        doReturn(CompletableFuture.completedFuture(mockResponseFrame)).when(nettyClient).invokeAsync(any(SbpFrame.class), any(Duration.class));

        List<DescribeStreamResultT> describeStreamResultTList = resourceManager.describeStreams(Collections.singletonList(streamId), DEFAULT_REQUEST_TIMEOUT).get();

        StreamT describedStream = describeStreamResultTList.get(0).getStream();
        assertEquals(streamId, describedStream.getStreamId());
        assertEquals(streamT.getReplicaNums(), describedStream.getReplicaNums());
        assertEquals(streamT.getRetentionPeriodMs(), describedStream.getRetentionPeriodMs());
    }

    @Test
    void createStreams() throws ExecutionException, InterruptedException {
        long streamId = 5;
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

        CreateStreamsResponseT createStreamsResponseT = new CreateStreamsResponseT();
        createStreamsResponseT.setStatus(okStatus);
        createStreamsResponseT.setThrottleTimeMs(0);
        createStreamsResponseT.setCreateResponses(new CreateStreamResultT[] {createStreamResultT});
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int createStreamsResponseOffset = CreateStreamsResponse.pack(builder, createStreamsResponseT);
        builder.finish(createStreamsResponseOffset);

        SbpFrame mockResponseFrame = SbpFrame.newBuilder()
            .setFlag((byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG))
            .setOperationCode(OperationCode.CREATE_STREAMS.getCode())
            .setHeader(builder.dataBuffer())
            .build();
        doReturn(CompletableFuture.completedFuture(mockResponseFrame)).when(nettyClient).invokeAsync(any(SbpFrame.class), any(Duration.class));

        List<CreateStreamResultT> resultList = resourceManager.createStreams(Collections.singletonList(streamT), DEFAULT_REQUEST_TIMEOUT).get();
        assertEquals(1, resultList.size());
        CreateStreamResultT result = resultList.get(0);
        assertEquals(okStatus.getCode(), result.getStatus().getCode());
        assertEquals(streamId, result.getStream().getStreamId());
        assertEquals(replicaNum, result.getStream().getReplicaNums());
        assertEquals(retentionPeriodMs, result.getStream().getRetentionPeriodMs());
        assertEquals(streamId, result.getRange().getStreamId());
        assertEquals(0, result.getRange().getRangeIndex());
        assertEquals(0, result.getRange().getStartOffset());
        assertEquals(-1, result.getRange().getEndOffset());
        assertEquals(0, result.getRange().getNextOffset());
        assertEquals(1, result.getRange().getReplicaNodes().length);
        assertEquals(replicaNodeT.getDataNode().getNodeId(), result.getRange().getReplicaNodes()[0].getDataNode().getNodeId());
        assertEquals(replicaNodeT.getDataNode().getAdvertiseAddr(), result.getRange().getReplicaNodes()[0].getDataNode().getAdvertiseAddr());
        assertEquals(replicaNodeT.getIsPrimary(), result.getRange().getReplicaNodes()[0].getIsPrimary());
    }
}