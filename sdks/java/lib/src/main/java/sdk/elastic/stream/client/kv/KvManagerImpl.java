package sdk.elastic.stream.client.kv;

import com.google.common.util.concurrent.ListenableFuture;
import com.google.protobuf.ByteString;
import io.grpc.Channel;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Iterator;
import java.util.Map;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import sdk.elastic.stream.apis.manager.KvManager;
import sdk.elastic.stream.grpc.kv.EventType;
import sdk.elastic.stream.grpc.kv.Item;
import sdk.elastic.stream.grpc.kv.KVGrpc;
import sdk.elastic.stream.grpc.kv.LoadRequest;
import sdk.elastic.stream.grpc.kv.LoadResponse;
import sdk.elastic.stream.grpc.kv.StoreRequest;
import sdk.elastic.stream.grpc.kv.StoreResponse;

public class KvManagerImpl implements KvManager {
    private static final Logger log = LoggerFactory.getLogger(KvManagerImpl.class);

    private final KVGrpc.KVFutureStub kvFutureStub;

    public KvManagerImpl(Channel channel) {
        this.kvFutureStub = KVGrpc.newFutureStub(channel);
    }

    @Override
    public ListenableFuture<StoreResponse> save(Map<String, ByteBuffer> kvMap, ByteBuffer prefix) {
        StoreRequest.Builder requestBuilder = StoreRequest.newBuilder();
        kvMap.forEach((key, valueBuffer) -> {
            Item item = Item.newBuilder()
                .setKind(EventType.PUT)
                .setName(ByteString.copyFrom(key.getBytes(StandardCharsets.ISO_8859_1)))
                .setPayload(ByteString.copyFrom(valueBuffer))
                .build();
            requestBuilder.addChanges(item);
        });
        if (prefix != null) {
            requestBuilder.setPrefix(ByteString.copyFrom(prefix));
        }

        return kvFutureStub.store(requestBuilder.build());
    }

    @Override
    public ListenableFuture<StoreResponse> delete(Iterator<String> keys, ByteBuffer prefix) {
        StoreRequest.Builder requestBuilder = StoreRequest.newBuilder();
        while (keys.hasNext()) {
            String key = keys.next();
            Item item = Item.newBuilder()
                .setKind(EventType.DELETE)
                .setName(ByteString.copyFrom(key.getBytes(StandardCharsets.ISO_8859_1)))
                .build();
            requestBuilder.addChanges(item);
        }
        if (prefix != null) {
            requestBuilder.setPrefix(ByteString.copyFrom(prefix));
        }

        return kvFutureStub.store(requestBuilder.build());
    }

    @Override
    public ListenableFuture<LoadResponse> load(Iterator<String> keys, ByteBuffer prefix) {
        LoadRequest.Builder requestBuilder = LoadRequest.newBuilder();
        while (keys.hasNext()) {
            String key = keys.next();
            requestBuilder.addNames(ByteString.copyFrom(key.getBytes(StandardCharsets.ISO_8859_1)));
        }
        if (prefix != null) {
            requestBuilder.setPrefix(ByteString.copyFrom(prefix));
        }

        return kvFutureStub.load(requestBuilder.build());
    }

}
