package com.automq.elasticstream.client;

import com.google.flatbuffers.FlatBufferBuilder;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.PooledByteBufAllocator;
import io.netty.buffer.Unpooled;
import com.automq.elasticstream.client.api.RecordBatch;
import com.automq.elasticstream.client.api.RecordBatchWithContext;
import com.automq.elasticstream.client.flatc.records.KeyValue;
import com.automq.elasticstream.client.flatc.records.RecordBatchMeta;
import com.automq.elasticstream.client.flatc.records.RecordBatchMetaT;

import java.nio.ByteBuffer;
import java.util.LinkedList;
import java.util.List;
import java.util.Map;

/**
 * RecordBatch =>
 * Magic => Int8
 * MetaLength => Int32
 * Meta => RecordBatchMeta
 * PayloadLength => Int32
 * BatchPayload => Bytes
 */
public class FlatRecordBatchCodec {
    private static final byte MAGIC_V0 = 0x22;
    private static final ThreadLocal<ByteBuffer> META_BUF = ThreadLocal.withInitial(() -> ByteBuffer.allocate(4096));
    private static final PooledByteBufAllocator ALLOCATOR = PooledByteBufAllocator.DEFAULT;

    /**
     * Encode RecordBatch to storage format record.
     *
     * @param recordBatch {@link RecordBatch}
     * @return storage format record bytes.
     */
    public static ByteBuf encode(long streamId, RecordBatch recordBatch) {

        int totalLength = 0;

        totalLength += 1; // magic

        FlatBufferBuilder metaBuilder = new FlatBufferBuilder(META_BUF.get());

        Integer propsVector = null;
        int propsSize = recordBatch.properties().size();
        if (propsSize > 0) {
            int[] kvs = new int[propsSize];
            int index = 0;
            for (Map.Entry<String, String> kv : recordBatch.properties().entrySet()) {
                int k = metaBuilder.createString(kv.getKey());
                int v = metaBuilder.createString(kv.getValue());
                int kv_ptr = KeyValue.createKeyValue(metaBuilder, k, v);
                kvs[index++] = kv_ptr;
            }
            propsVector = RecordBatchMeta.createPropertiesVector(metaBuilder, kvs);
        }

        // encode RecordBatchMeta
        RecordBatchMeta.startRecordBatchMeta(metaBuilder);
        RecordBatchMeta.addStreamId(metaBuilder, streamId);
        RecordBatchMeta.addLastOffsetDelta(metaBuilder, recordBatch.count());
        RecordBatchMeta.addBaseTimestamp(metaBuilder, recordBatch.baseTimestamp());
        if (null != propsVector) {
            RecordBatchMeta.addProperties(metaBuilder, propsVector);
        }
        int ptr = RecordBatchMeta.endRecordBatchMeta(metaBuilder);
        metaBuilder.finish(ptr);

        // The data in this ByteBuffer does NOT start at 0, but at buf.position().
        // The number of bytes is buf.remaining().
        ByteBuffer metaBuf = metaBuilder.dataBuffer();

        totalLength += 4; // meta length
        totalLength += metaBuf.remaining(); // RecordBatchMeta

        totalLength += 4; // payload length
        totalLength += recordBatch.rawPayload().remaining(); // payload

        ByteBuf buf = ALLOCATOR.directBuffer(totalLength);
        buf.writeByte(MAGIC_V0); // magic
        buf.writeInt(metaBuf.remaining()); // meta length
        buf.writeBytes(metaBuf); // RecordBatchMeta
        buf.writeInt(recordBatch.rawPayload().remaining()); // payload length
        buf.writeBytes(recordBatch.rawPayload()); // payload

        META_BUF.get().clear();
        return buf;
    }

    /**
     * Decode storage format record to RecordBatchWithContext list.
     *
     * @param storageFormatBytes storage format bytes.
     * @return RecordBatchWithContext list.
     */
    public static List<RecordBatchWithContext> decode(ByteBuffer storageFormatBytes) {
        ByteBuf buf = Unpooled.wrappedBuffer(storageFormatBytes);
        List<RecordBatchWithContext> recordBatchList = new LinkedList<>();
        while (buf.isReadable()) {
            buf.readByte(); // magic
            int metaLength = buf.readInt();
            ByteBuf metaBuf = buf.slice(buf.readerIndex(), metaLength);
            RecordBatchMetaT recordBatchMetaT = RecordBatchMeta.getRootAsRecordBatchMeta(metaBuf.nioBuffer()).unpack();
            buf.skipBytes(metaLength);
            int payloadLength = buf.readInt();
            ByteBuf payloadBuf = buf.slice(buf.readerIndex(), payloadLength);
            buf.skipBytes(payloadLength);
            recordBatchList.add(new FlatRecordBatchWithContext(recordBatchMetaT, payloadBuf.nioBuffer()));
        }
        return recordBatchList;
    }

}
