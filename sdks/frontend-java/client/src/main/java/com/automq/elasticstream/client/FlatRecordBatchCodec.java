package com.automq.elasticstream.client;

import com.google.flatbuffers.FlatBufferBuilder;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.PooledByteBufAllocator;
import io.netty.buffer.Unpooled;
import com.automq.elasticstream.client.api.RecordBatch;
import com.automq.elasticstream.client.api.RecordBatchWithContext;
import com.automq.elasticstream.client.flatc.records.KeyValueT;
import com.automq.elasticstream.client.flatc.records.RecordBatchMeta;
import com.automq.elasticstream.client.flatc.records.RecordBatchMetaT;

import java.nio.ByteBuffer;
import java.util.LinkedList;
import java.util.List;

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

        // encode RecordBatchMeta
        FlatBufferBuilder metaBuilder = new FlatBufferBuilder();
        RecordBatchMetaT recordBatchMetaT = new RecordBatchMetaT();
        recordBatchMetaT.setStreamId(streamId);
        recordBatchMetaT.setLastOffsetDelta(recordBatch.count());
        recordBatchMetaT.setBaseTimestamp(recordBatch.baseTimestamp());
        recordBatchMetaT.setProperties(recordBatch.properties().entrySet().stream().map(entry -> {
            KeyValueT kv = new KeyValueT();
            kv.setKey(entry.getKey());
            kv.setValue(entry.getValue());
            return kv;
        }).toArray(KeyValueT[]::new));
        metaBuilder.finish(RecordBatchMeta.pack(metaBuilder, recordBatchMetaT));
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
