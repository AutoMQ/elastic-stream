package sdk.elastic.stream.integration.client.impl;

import io.netty.util.HashedWheelTimer;
import java.time.Duration;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.ExecutionException;
import org.junit.jupiter.api.AfterAll;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import sdk.elastic.stream.apis.ClientConfigurationBuilder;
import sdk.elastic.stream.apis.manager.ResourceManager;
import sdk.elastic.stream.client.impl.ResourceManagerImpl;
import sdk.elastic.stream.client.netty.NettyClient;
import sdk.elastic.stream.client.route.Address;
import sdk.elastic.stream.flatc.header.CreateStreamResultT;
import sdk.elastic.stream.flatc.header.DataNodeT;
import sdk.elastic.stream.flatc.header.DescribeRangeResultT;
import sdk.elastic.stream.flatc.header.DescribeStreamResultT;
import sdk.elastic.stream.flatc.header.ErrorCode;
import sdk.elastic.stream.flatc.header.RangeCriteriaT;
import sdk.elastic.stream.flatc.header.RangeIdT;
import sdk.elastic.stream.flatc.header.RangeT;
import sdk.elastic.stream.flatc.header.ReplicaNodeT;
import sdk.elastic.stream.flatc.header.SealRangesResultT;
import sdk.elastic.stream.flatc.header.StreamT;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static sdk.elastic.stream.client.protocol.SbpFrame.ENDING_RESPONSE_FLAG;
import static sdk.elastic.stream.client.protocol.SbpFrame.GENERAL_RESPONSE_FLAG;

class ResourceManagerImplTest {
    private static final Address defaultPmAddress = Address.fromAddress(System.getProperty("pm.address"));
    private static final HashedWheelTimer timer = new HashedWheelTimer(r -> new Thread(r, "TestHouseKeepingService"));
    private static final Duration DEFAULT_REQUEST_TIMEOUT = Duration.ofSeconds(3);
    private static ResourceManager resourceManager;
    private static NettyClient nettyClient;

    @BeforeAll
    public static void setUp() throws Exception {
        ClientConfigurationBuilder builder = new ClientConfigurationBuilder()
            .setPmEndpoint(String.format("%s:%d", defaultPmAddress.getHost(), defaultPmAddress.getPort()))
            .setConnectionTimeout(Duration.ofSeconds(3))
            .setChannelMaxIdleTime(Duration.ofSeconds(10))
            .setHeartBeatInterval(Duration.ofSeconds(9));
        nettyClient = new NettyClient(builder.build(), timer);
        nettyClient.start();
        resourceManager = new ResourceManagerImpl(nettyClient);
    }

    @Test
    public void pingPong() throws ExecutionException, InterruptedException {
        Byte flag = resourceManager.pingPong(DEFAULT_REQUEST_TIMEOUT).get();
        assertEquals(flag, (byte) (GENERAL_RESPONSE_FLAG | ENDING_RESPONSE_FLAG));
    }

    @Test
    public void pingPongFail() {
        nettyClient.updatePmAddress(new Address("1.1.1.1", 80));
        assertThrows(ExecutionException.class, () -> resourceManager.pingPong(DEFAULT_REQUEST_TIMEOUT).get());
    }

    @Test
    public void createStreams() throws ExecutionException, InterruptedException {
        StreamT streamT = new StreamT();
        streamT.setStreamId(0L);
        streamT.setReplicaNum((byte) 1);
        streamT.setRetentionPeriodMs(Duration.ofDays(3).toMillis());
        List<StreamT> streamTList = new ArrayList<>(1);
        streamTList.add(streamT);
        List<CreateStreamResultT> resultList = resourceManager.createStreams(streamTList, DEFAULT_REQUEST_TIMEOUT).get();
        assertEquals(1, resultList.size());

        StreamT resultStream = resultList.get(0).getStream();
        assertTrue(resultStream.getStreamId() > 0);
        assertEquals(1, resultStream.getReplicaNum());
        assertEquals(Duration.ofDays(3).toMillis(), resultStream.getRetentionPeriodMs());

        System.out.println(resultStream.getStreamId());
        System.out.println(resultStream.getReplicaNum());
    }

    @Test
    public void listRanges() throws ExecutionException, InterruptedException {
        StreamT resultStream = createOneStream().getStream();

        RangeCriteriaT rangeCriteriaT = new RangeCriteriaT();
        rangeCriteriaT.setStreamId(resultStream.getStreamId());
        RangeT[] ranges = resourceManager.listRanges(Collections.singletonList(rangeCriteriaT), Duration.ofSeconds(3))
            .thenApply(list -> list.get(0).getRanges()).get();
        assertNotNull(ranges);
        assertEquals(1, ranges.length);
        assertEquals(0, ranges[0].getStartOffset());
        // not sealed.
        assertTrue(ranges[0].getEndOffset() <= 0);
        assertEquals(0, ranges[0].getNextOffset());

        int primaryNodeNum = 0;
        for (ReplicaNodeT node : ranges[0].getReplicaNodes()) {
            if (node.getIsPrimary()) {
                primaryNodeNum += 1;
            }
        }
        // There is one and only one primary data node.
        assertEquals(1, primaryNodeNum);
    }

    @Test
    public void sealRanges() throws ExecutionException, InterruptedException {
        StreamT resultStream = createOneStream().getStream();

        RangeIdT rangeIdT = new RangeIdT();
        rangeIdT.setStreamId(resultStream.getStreamId());
        rangeIdT.setRangeIndex(0);

        SealRangesResultT result = resourceManager.sealRanges(Collections.singletonList(rangeIdT), DEFAULT_REQUEST_TIMEOUT)
            .join()
            .get(0);
        assertEquals(ErrorCode.OK, result.getStatus().getCode());
        assertEquals(resultStream.getStreamId(), result.getRange().getStreamId());
        assertEquals(0, result.getRange().getStartOffset());
        // Server will ignore the seal request if start = end.
        assertTrue(result.getRange().getEndOffset() <= 0);
    }

    @Test
    public void describeRanges() throws ExecutionException, InterruptedException {
        StreamT resultStream = createOneStream().getStream();

        RangeCriteriaT rangeCriteriaT = new RangeCriteriaT();
        rangeCriteriaT.setStreamId(resultStream.getStreamId());
        RangeT range = resourceManager.listRanges(Collections.singletonList(rangeCriteriaT), Duration.ofSeconds(3))
            .thenApply(list -> list.get(0).getRanges()).get()[0];

        DataNodeT dataNode = range.getReplicaNodes()[0].getDataNode();
        RangeIdT rangeIdT = new RangeIdT();
        rangeIdT.setRangeIndex(range.getRangeIndex());
        rangeIdT.setStreamId(range.getStreamId());
        List<DescribeRangeResultT> describeRangeResultTList = resourceManager.describeRanges(Address.fromAddress(dataNode.getAdvertiseAddr()), Collections.singletonList(rangeIdT), DEFAULT_REQUEST_TIMEOUT).get();

        RangeT describedRange = describeRangeResultTList.get(0).getRange();
        assertEquals(describedRange.getRangeIndex(), range.getRangeIndex());
        assertEquals(describedRange.getStreamId(), range.getStreamId());
        assertEquals(describedRange.getStartOffset(), range.getStartOffset());
        assertEquals(describedRange.getEndOffset(), range.getEndOffset());
        assertEquals(describedRange.getNextOffset(), range.getNextOffset());
    }

    @Test
    public void describeStreams() throws ExecutionException, InterruptedException {
        StreamT resultStream = createOneStream().getStream();

        List<DescribeStreamResultT> describeStreamResultTList = resourceManager.describeStreams(Collections.singletonList(resultStream.getStreamId()), DEFAULT_REQUEST_TIMEOUT).get();

        StreamT describedStream = describeStreamResultTList.get(0).getStream();
        assertEquals(describedStream.getStreamId(), resultStream.getStreamId());
        assertEquals(describedStream.getReplicaNum(), resultStream.getReplicaNum());
        assertEquals(describedStream.getRetentionPeriodMs(), resultStream.getRetentionPeriodMs());
    }

    private CreateStreamResultT createOneStream() throws ExecutionException, InterruptedException {
        StreamT streamT = new StreamT();
        streamT.setStreamId(0L);
        streamT.setReplicaNum((byte) 1);
        streamT.setRetentionPeriodMs(Duration.ofDays(3).toMillis());
        List<StreamT> streamTList = new ArrayList<>(1);
        streamTList.add(streamT);
        return resourceManager.createStreams(streamTList, DEFAULT_REQUEST_TIMEOUT)
            .get()
            .get(0);
    }

    @AfterAll
    public static void tearDown() throws Exception {
        timer.stop();
        nettyClient.close();
    }
}