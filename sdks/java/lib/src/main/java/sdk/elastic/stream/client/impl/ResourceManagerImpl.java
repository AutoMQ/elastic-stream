package sdk.elastic.stream.client.impl;

import com.google.common.base.Preconditions;
import com.google.flatbuffers.FlatBufferBuilder;
import java.time.Duration;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import sdk.elastic.stream.apis.exception.ClientException;
import sdk.elastic.stream.apis.exception.RetryableException;
import sdk.elastic.stream.apis.manager.ResourceManager;
import sdk.elastic.stream.client.common.ProtocolUtil;
import sdk.elastic.stream.client.netty.NettyClient;
import sdk.elastic.stream.client.protocol.SbpFrame;
import sdk.elastic.stream.client.protocol.StatusTDecorator;
import sdk.elastic.stream.client.route.Address;
import sdk.elastic.stream.flatc.header.CreateStreamResultT;
import sdk.elastic.stream.flatc.header.CreateStreamsRequest;
import sdk.elastic.stream.flatc.header.CreateStreamsRequestT;
import sdk.elastic.stream.flatc.header.CreateStreamsResponse;
import sdk.elastic.stream.flatc.header.DescribeRangeResultT;
import sdk.elastic.stream.flatc.header.DescribeRangesRequest;
import sdk.elastic.stream.flatc.header.DescribeRangesRequestT;
import sdk.elastic.stream.flatc.header.DescribeRangesResponse;
import sdk.elastic.stream.flatc.header.DescribeStreamResultT;
import sdk.elastic.stream.flatc.header.DescribeStreamsRequest;
import sdk.elastic.stream.flatc.header.DescribeStreamsRequestT;
import sdk.elastic.stream.flatc.header.DescribeStreamsResponse;
import sdk.elastic.stream.flatc.header.ListRangesRequest;
import sdk.elastic.stream.flatc.header.ListRangesRequestT;
import sdk.elastic.stream.flatc.header.ListRangesResponse;
import sdk.elastic.stream.flatc.header.ListRangesResultT;
import sdk.elastic.stream.flatc.header.RangeCriteriaT;
import sdk.elastic.stream.flatc.header.RangeIdT;
import sdk.elastic.stream.flatc.header.SealRangesRequest;
import sdk.elastic.stream.flatc.header.SealRangesRequestT;
import sdk.elastic.stream.flatc.header.SealRangesResponse;
import sdk.elastic.stream.flatc.header.SealRangesResultT;
import sdk.elastic.stream.flatc.header.Status;
import sdk.elastic.stream.flatc.header.StreamT;
import sdk.elastic.stream.models.OperationCode;

import static sdk.elastic.stream.flatc.header.ErrorCode.OK;

public class ResourceManagerImpl implements ResourceManager {
    private static final Logger log = LoggerFactory.getLogger(ResourceManagerImpl.class);
    private final NettyClient nettyClient;

    public ResourceManagerImpl(NettyClient nettyClient) {
        this.nettyClient = nettyClient;
    }

    @Override
    public CompletableFuture<List<ListRangesResultT>> listRanges(List<RangeCriteriaT> rangeCriteriaList,
        Duration timeout) {
        Preconditions.checkArgument(rangeCriteriaList != null && rangeCriteriaList.size() > 0,
            "Invalid range criteria list since no range criteria was found.");

        ListRangesRequestT listRangesRequestT = new ListRangesRequestT();
        listRangesRequestT.setRangeCriteria(rangeCriteriaList.toArray(new RangeCriteriaT[0]));
        listRangesRequestT.setTimeoutMs((int) timeout.toMillis());
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int listRangesRequestOffset = ListRangesRequest.pack(builder, listRangesRequestT);
        builder.finish(listRangesRequestOffset);

        SbpFrame sbpFrame = ProtocolUtil.constructRequestSbpFrame(OperationCode.LIST_RANGES, builder.dataBuffer());
        return listRanges0(sbpFrame, timeout).exceptionally(throwable -> {
            if (throwable instanceof RetryableException) {
                return listRanges0(sbpFrame, timeout).join();
            }
            return null;
        });
    }

    private CompletableFuture<List<ListRangesResultT>> listRanges0(SbpFrame sbpFrame,
        Duration timeout) {
        return nettyClient.invokeAsync(sbpFrame, timeout)
            .thenCompose(responseFrame -> {
                ListRangesResponse response = ListRangesResponse.getRootAsListRangesResponse(responseFrame.getHeader());

                CompletableFuture<List<ListRangesResultT>> completableFuture = new CompletableFuture<>();

                // need to connect to new Pm primary node.
                if (maybeUpdatePmAddress(response.status())) {
                    completableFuture.completeExceptionally(new RetryableException("Pm address changed"));
                    return completableFuture;
                }

                if (response.status().code() != OK) {
                    completableFuture.completeExceptionally(new ClientException("List ranges failed with code " + response.status().code() + ", msg: " + response.status().message()));
                    return completableFuture;
                }

                completableFuture.complete(Arrays.asList(response.unpack().getListResponses()));
                return completableFuture;
            });
    }

    @Override
    public CompletableFuture<Byte> pingPong(Duration timeout) {
        SbpFrame sbpFrame = ProtocolUtil.constructRequestSbpFrame(OperationCode.PING, null);
        return nettyClient.invokeAsync(sbpFrame, timeout)
            .thenApply(SbpFrame::getFlag);
    }

    @Override
    public CompletableFuture<List<SealRangesResultT>> sealRanges(List<RangeIdT> rangeIdList, Duration timeout) {
        Preconditions.checkArgument(rangeIdList != null && rangeIdList.size() > 0, "Invalid range id list since no range id was found.");

        SealRangesRequestT sealRangesRequestT = new SealRangesRequestT();
        sealRangesRequestT.setRanges(rangeIdList.toArray(new RangeIdT[0]));
        sealRangesRequestT.setTimeoutMs((int) timeout.toMillis());
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int sealRangesRequestOffset = SealRangesRequest.pack(builder, sealRangesRequestT);
        builder.finish(sealRangesRequestOffset);

        SbpFrame sbpFrame = ProtocolUtil.constructRequestSbpFrame(OperationCode.SEAL_RANGES, builder.dataBuffer());
        return sealRanges0(sbpFrame, timeout).exceptionally(throwable -> {
            if (throwable instanceof RetryableException) {
                return sealRanges0(sbpFrame, timeout).join();
            }
            return null;
        });
    }

    private CompletableFuture<List<SealRangesResultT>> sealRanges0(SbpFrame sbpFrame, Duration timeout) {
        return nettyClient.invokeAsync(sbpFrame, timeout)
            .thenCompose(responseFrame -> {
                SealRangesResponse response = SealRangesResponse.getRootAsSealRangesResponse(responseFrame.getHeader());
                CompletableFuture<List<SealRangesResultT>> completableFuture = new CompletableFuture<>();

                // need to connect to new Pm primary node.
                if (maybeUpdatePmAddress(response.status())) {
                    completableFuture.completeExceptionally(new RetryableException("Pm address changed"));
                    return completableFuture;
                }
                if (response.status().code() != OK) {
                    completableFuture.completeExceptionally(new ClientException("Seal ranges failed with code " + response.status().code() + ", msg: " + response.status().message()));
                    return completableFuture;
                }
                completableFuture.complete(Arrays.asList(response.unpack().getSealResponses()));
                return completableFuture;
            });
    }

    @Override
    public CompletableFuture<List<DescribeRangeResultT>> describeRanges(Address dataNodeAddress,
        List<RangeIdT> rangeIdList, Duration timeout) {
        Preconditions.checkArgument(rangeIdList != null && rangeIdList.size() > 0, "Invalid range id list since no range id was found.");

        DescribeRangesRequestT describeRangesRequestT = new DescribeRangesRequestT();
        describeRangesRequestT.setRanges(rangeIdList.toArray(new RangeIdT[0]));
        describeRangesRequestT.setTimeoutMs((int) timeout.toMillis());
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int describeRangesRequestOffset = DescribeRangesRequest.pack(builder, describeRangesRequestT);
        builder.finish(describeRangesRequestOffset);

        SbpFrame sbpFrame = ProtocolUtil.constructRequestSbpFrame(OperationCode.DESCRIBE_RANGES, builder.dataBuffer());
        return nettyClient.invokeAsync(dataNodeAddress, sbpFrame, timeout)
            .thenCompose(responseFrame -> {
                DescribeRangesResponse response = DescribeRangesResponse.getRootAsDescribeRangesResponse(responseFrame.getHeader());
                CompletableFuture<List<DescribeRangeResultT>> completableFuture = new CompletableFuture<>();
                if (response.status().code() != OK) {
                    completableFuture.completeExceptionally(new ClientException("Describe ranges failed with code " + response.status().code() + ", msg: " + response.status().message()));
                    return completableFuture;
                }
                completableFuture.complete(Arrays.asList(response.unpack().getDescribeResponses()));
                return completableFuture;
            });
    }

    @Override
    public CompletableFuture<List<DescribeStreamResultT>> describeStreams(List<Long> streamIdList, Duration timeout) {
        Preconditions.checkArgument(streamIdList != null && streamIdList.size() > 0, "Invalid stream id list since no stream id was found.");

        DescribeStreamsRequestT describeStreamsRequestT = new DescribeStreamsRequestT();
        describeStreamsRequestT.setStreamIds(streamIdList.stream().mapToLong(Long::longValue).toArray());
        describeStreamsRequestT.setTimeoutMs((int) timeout.toMillis());
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int describeStreamsRequestOffset = DescribeStreamsRequest.pack(builder, describeStreamsRequestT);
        builder.finish(describeStreamsRequestOffset);

        SbpFrame sbpFrame = ProtocolUtil.constructRequestSbpFrame(OperationCode.DESCRIBE_STREAMS, builder.dataBuffer());
        return describeStreams0(sbpFrame, timeout).exceptionally(throwable -> {
            if (throwable instanceof RetryableException) {
                return describeStreams0(sbpFrame, timeout).join();
            }
            return null;
        });
    }

    private CompletableFuture<List<DescribeStreamResultT>> describeStreams0(SbpFrame sbpFrame, Duration timeout) {
        return nettyClient.invokeAsync(sbpFrame, timeout)
            .thenCompose(responseFrame -> {
                DescribeStreamsResponse response = DescribeStreamsResponse.getRootAsDescribeStreamsResponse(responseFrame.getHeader());
                CompletableFuture<List<DescribeStreamResultT>> completableFuture = new CompletableFuture<>();
                // need to connect to new Pm primary node.
                if (maybeUpdatePmAddress(response.status())) {
                    completableFuture.completeExceptionally(new RetryableException("Pm address changed"));
                    return completableFuture;
                }
                if (response.status().code() != OK) {
                    completableFuture.completeExceptionally(new ClientException("Describe streams failed with code " + response.status().code() + ", msg: " + response.status().message()));
                    return completableFuture;
                }
                completableFuture.complete(Arrays.asList(response.unpack().getDescribeResponses()));
                return completableFuture;
            });
    }

    @Override
    public CompletableFuture<List<CreateStreamResultT>> createStreams(List<StreamT> streams, Duration timeout) {
        Preconditions.checkArgument(streams != null && streams.size() > 0, "Invalid streams since no streams were found.");

        CreateStreamsRequestT createStreamsRequestT = new CreateStreamsRequestT();
        createStreamsRequestT.setStreams(streams.toArray(new StreamT[0]));
        createStreamsRequestT.setTimeoutMs((int) timeout.toMillis());
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int createStreamsRequestOffset = CreateStreamsRequest.pack(builder, createStreamsRequestT);
        builder.finish(createStreamsRequestOffset);

        SbpFrame sbpFrame = ProtocolUtil.constructRequestSbpFrame(OperationCode.CREATE_STREAMS, builder.dataBuffer());
        return createStreams0(sbpFrame, timeout).exceptionally(throwable -> {
            if (throwable instanceof RetryableException) {
                return createStreams0(sbpFrame, timeout).join();
            }
            return null;
        });
    }

    private CompletableFuture<List<CreateStreamResultT>> createStreams0(SbpFrame sbpFrame, Duration timeout) {
        return nettyClient.invokeAsync(sbpFrame, timeout)
            .thenCompose(responseFrame -> {
                CreateStreamsResponse response = CreateStreamsResponse.getRootAsCreateStreamsResponse(responseFrame.getHeader());
                CompletableFuture<List<CreateStreamResultT>> completableFuture = new CompletableFuture<>();

                // need to connect to new Pm primary node.
                if (maybeUpdatePmAddress(response.status())) {
                    completableFuture.completeExceptionally(new RetryableException("Pm address changed"));
                    return completableFuture;
                }
                if (response.status().code() != OK) {
                    completableFuture.completeExceptionally(new ClientException("Create streams failed with code " + response.status().code() + ", msg: " + response.status().message()));
                    return completableFuture;
                }
                completableFuture.complete(Arrays.asList(response.unpack().getCreateResponses()));
                return completableFuture;
            });
    }

    /**
     * Update pm address if needed.
     *
     * @param status status
     * @return true if pm address is updated, false otherwise.
     */
    public boolean maybeUpdatePmAddress(Status status) {
        Address updatePmAddress = new StatusTDecorator(status).maybeGetNewPmAddress();
        // need to connect to new Pm primary node.
        if (updatePmAddress != null) {
            nettyClient.updatePmAddress(updatePmAddress);
            return true;
        }
        return false;
    }
}
