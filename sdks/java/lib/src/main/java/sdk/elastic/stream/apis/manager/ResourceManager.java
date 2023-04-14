package sdk.elastic.stream.apis.manager;

import java.time.Duration;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import sdk.elastic.stream.client.route.Address;
import sdk.elastic.stream.flatc.header.CreateStreamResultT;
import sdk.elastic.stream.flatc.header.DescribeRangeResultT;
import sdk.elastic.stream.flatc.header.DescribeStreamResultT;
import sdk.elastic.stream.flatc.header.ListRangesResultT;
import sdk.elastic.stream.flatc.header.RangeCriteriaT;
import sdk.elastic.stream.flatc.header.RangeIdT;
import sdk.elastic.stream.flatc.header.SealRangesResultT;
import sdk.elastic.stream.flatc.header.StreamT;

public interface ResourceManager {
    /**
     * List the ranges of a batch of streams or the ranges of all the streams in a specific data node.
     * Null will be returned if the request failed.
     *
     * @param rangeCriteriaList This could be a data node or a list of streams.
     * @param timeout           request timeout.
     * @return
     */
    CompletableFuture<List<ListRangesResultT>> listRanges(List<RangeCriteriaT> rangeCriteriaList, Duration timeout);

    /**
     * Seal specific ranges. If the range is sealed, no more data can be appended to it.
     *
     * @param rangeIdList range id list.
     * @param timeout     request timeout.
     * @return seal ranges results.
     */
    CompletableFuture<List<SealRangesResultT>> sealRanges(List<RangeIdT> rangeIdList, Duration timeout);

    /**
     * Describe the ranges of a batch of streams.
     * It is often used to get the recent end offset of the stream after the write operation.
     *
     * @param dataNodeAddress any valid data node address.
     * @param rangeIdList     range id list.
     * @param timeout         request timeout.
     * @return describe ranges results.
     */
    CompletableFuture<List<DescribeRangeResultT>> describeRanges(Address dataNodeAddress, List<RangeIdT> rangeIdList,
        Duration timeout);

    /**
     * Create a batch of streams.
     *
     * @param streams A batch of streams to be created.
     * @param timeout request timeout.
     * @return create stream results.
     */
    CompletableFuture<List<CreateStreamResultT>> createStreams(List<StreamT> streams, Duration timeout);

    /**
     * Get specific streams' general info.
     *
     * @param streamIdList stream id list.
     * @param timeout      request timeout.
     * @return describe streams results.
     */
    CompletableFuture<List<DescribeStreamResultT>> describeStreams(List<Long> streamIdList, Duration timeout);

    /**
     * Ping the PM.
     *
     * @param timeout request timeout.
     * @return pong flag.
     */
    CompletableFuture<Byte> pingPong(Duration timeout);

}
