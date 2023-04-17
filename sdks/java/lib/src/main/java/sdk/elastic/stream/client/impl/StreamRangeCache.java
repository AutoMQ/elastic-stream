package sdk.elastic.stream.client.impl;

import com.google.common.cache.CacheLoader;
import java.util.AbstractMap;
import java.util.TreeMap;
import java.util.concurrent.CompletableFuture;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import sdk.elastic.stream.apis.exception.ClientException;
import sdk.elastic.stream.client.cache.CommonCache;
import sdk.elastic.stream.flatc.header.RangeT;

/**
 * StreamRangeCache is a cache for StreamId to StreamRanges mapping.
 */
public class StreamRangeCache extends CommonCache<Long, TreeMap<Long, RangeT>> {
    private static final Logger log = LoggerFactory.getLogger(StreamRangeCache.class);
    private static final int DEFAULT_CACHE_SIZE = 1000;

    /**
     * Create a new StreamRangeCache with the specified size and CacheLoader.
     *
     * @param size   - the maximum number of entries the cache may contain
     * @param loader - the CacheLoader used to compute values
     */
    public StreamRangeCache(int size, CacheLoader<Long, TreeMap<Long, RangeT>> loader) {
        super(size, loader);
    }

    /**
     * Create a new StreamRangeCache with the specified CacheLoader.
     * The cache size is set to the default value.
     *
     * @param loader - the CacheLoader used to compute values
     */
    public StreamRangeCache(CacheLoader<Long, TreeMap<Long, RangeT>> loader) {
        this(DEFAULT_CACHE_SIZE, loader);
    }

    /**
     * Put stream ranges by stream id.
     * If streamId is null or ranges is null or ranges is empty, do nothing.
     *
     * @param streamId - with which the specified ranges is to be associated
     * @param ranges   - the ranges to be associated with the specified streamId
     */
    public void put(Long streamId, RangeT[] ranges) {
        if (streamId != null && ranges != null && ranges.length > 0) {
            TreeMap<Long, RangeT> rangesMap = new TreeMap<>();
            for (RangeT range : ranges) {
                rangesMap.put(range.getStartOffset(), range);
            }
            super.put(streamId, rangesMap);
        }
    }

    /**
     * Get the floor range, which contains valid recordBatch based on the given startOffset.
     * This method is generally used to get the range for fetching.
     *
     * @param streamId    with which the specified ranges is to be associated
     * @param startOffset the offset to search
     * @return the floor range
     */
    public CompletableFuture<RangeT> getFloorRange(long streamId, long startOffset) {
        assert streamId > 0 && startOffset >= 0;
        CompletableFuture<RangeT> completableFuture = new CompletableFuture<>();

        try {
            TreeMap<Long, RangeT> rangesMap = get(streamId);
            AbstractMap.Entry<Long, RangeT> floorEntry = rangesMap.floorEntry(startOffset);

            if (floorEntry == null || floorEntry.getValue() == null) {
                completableFuture.completeExceptionally(new ClientException("Floor range is null for stream " + streamId + " with offset " + startOffset));
                return completableFuture;
            }

            completableFuture.complete(floorEntry.getValue());
        } catch (Throwable ex) {
            completableFuture.completeExceptionally(ex);
        }

        return completableFuture;
    }

    /**
     * Get the last range of the stream.
     *
     * @param streamId with which the specified ranges is to be associated
     * @return the last range
     */
    public CompletableFuture<RangeT> getLastRange(long streamId) {
        assert streamId > 0;
        CompletableFuture<RangeT> completableFuture = new CompletableFuture<>();

        try {
            RangeT targetRange = get(streamId).lastEntry().getValue();
            // Data nodes may have been ready. Try again.
            if (targetRange.getReplicaNodes().length == 0) {
                refresh(streamId);
                targetRange = get(streamId).lastEntry().getValue();
            }
            if (targetRange.getReplicaNodes().length == 0) {
                completableFuture.completeExceptionally(new ClientException("Failed to get ReplicaNodes of the last range of stream " + streamId));
                return completableFuture;
            }
            if (targetRange.getEndOffset() > 0) {
                log.error("The last range of stream {} is sealed, rangeId {}, startOffset {}, endOffset {}", streamId, targetRange.getRangeIndex(), targetRange.getStartOffset(), targetRange.getEndOffset());
                completableFuture.completeExceptionally(new ClientException("Get the invalid last range of stream " + streamId));
                return completableFuture;
            }
            completableFuture.complete(targetRange);
        } catch (Throwable ex) {
            completableFuture.completeExceptionally(ex);
        }
        return completableFuture;
    }
}
