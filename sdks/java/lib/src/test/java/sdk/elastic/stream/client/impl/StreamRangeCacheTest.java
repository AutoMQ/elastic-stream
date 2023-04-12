package sdk.elastic.stream.client.impl;

import com.google.common.cache.CacheLoader;
import java.util.TreeMap;
import java.util.concurrent.ExecutionException;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;
import sdk.elastic.stream.flatc.header.DataNodeT;
import sdk.elastic.stream.flatc.header.RangeT;
import sdk.elastic.stream.flatc.header.ReplicaNodeT;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

class StreamRangeCacheTest {
    private static ReplicaNodeT replicaNodeT;
    private static CacheLoader<Long, TreeMap<Long, RangeT>> emptyCacheLoader = new CacheLoader<Long, TreeMap<Long, RangeT>>() {
        @Override
        public TreeMap<Long, RangeT> load(Long key) throws Exception {
            return new TreeMap<>();
        }
    };

    @BeforeAll
    static void setUpAll() {
        DataNodeT dataNodeT = new DataNodeT();
        dataNodeT.setNodeId(1);
        dataNodeT.setAdvertiseAddr("localhost:8080");

        replicaNodeT = new ReplicaNodeT();
        replicaNodeT.setDataNode(dataNodeT);
        replicaNodeT.setIsPrimary(true);
    }

    @Test
    void getFloorRange() throws ExecutionException, InterruptedException {
        StreamRangeCache streamRangeCache = new StreamRangeCache(emptyCacheLoader);
        long streamId = 6L;
        RangeT[] rangeTArray = new RangeT[3];
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
        rangeTArray[1].setEndOffset(150);
        rangeTArray[1].setNextOffset(150);
        rangeTArray[2] = new RangeT();
        rangeTArray[2].setStreamId(streamId);
        rangeTArray[2].setReplicaNodes(new ReplicaNodeT[] {replicaNodeT});
        rangeTArray[2].setRangeIndex(2);
        rangeTArray[2].setStartOffset(150);
        rangeTArray[2].setEndOffset(-1);
        rangeTArray[2].setNextOffset(171);
        streamRangeCache.put(streamId, rangeTArray);

        RangeT rangeT = streamRangeCache.getFloorRange(streamId, 11).get();
        assertEquals(0, rangeT.getRangeIndex());
        rangeT = streamRangeCache.getFloorRange(streamId, 100).get();
        assertEquals(1, rangeT.getRangeIndex());
        rangeT = streamRangeCache.getFloorRange(streamId, 190).get();
        assertEquals(2, rangeT.getRangeIndex());
    }

    @Test
    void getLastRange() throws ExecutionException, InterruptedException {
        StreamRangeCache streamRangeCache = new StreamRangeCache(emptyCacheLoader);
        long streamId = 7L;
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
        rangeTArray[1].setStartOffset(120);
        rangeTArray[1].setEndOffset(-1);
        rangeTArray[1].setNextOffset(150);
        streamRangeCache.put(streamId, rangeTArray);

        RangeT rangeT = streamRangeCache.getLastRange(streamId).get();
        assertEquals(1, rangeT.getRangeIndex());
    }

    @Test
    void getLastRangeError() {
        StreamRangeCache streamRangeCache = new StreamRangeCache(emptyCacheLoader);
        long streamId = 8L;
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
        rangeTArray[1].setStartOffset(120);
        rangeTArray[1].setEndOffset(150);
        rangeTArray[1].setNextOffset(150);
        streamRangeCache.put(streamId, rangeTArray);

        assertThrows(ExecutionException.class, () -> streamRangeCache.getLastRange(streamId).get());
    }
}