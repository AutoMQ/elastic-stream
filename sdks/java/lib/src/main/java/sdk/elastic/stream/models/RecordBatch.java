package sdk.elastic.stream.models;

import com.google.common.base.Preconditions;
import com.google.flatbuffers.FlatBufferBuilder;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import sdk.elastic.stream.client.common.ProtocolUtil;
import sdk.elastic.stream.flatc.records.RecordBatchMeta;
import sdk.elastic.stream.flatc.records.RecordBatchMetaT;
import sdk.elastic.stream.flatc.records.RecordMeta;
import sdk.elastic.stream.flatc.records.RecordMetaT;

/**
 * A record batch wraps a list of records with the same stream id.
 * Note that data nodes store records with a whole {@link RecordBatch} instead of a single {@link Record}.
 * Besides, offsets of records in a record batch are in ascending order. However, timestamps of records in a record batch may be not.
 */
public class RecordBatch {
    /**
     * The minimum length of a record batch.
     */
    private static final int MIN_LENGTH = 13;
    private static final byte DEFAULT_MAGIC = 0x22;
    private static final short DEFAULT_FLAG = 0;
    private static final int MAX_OFFSET_DELTA = Integer.MAX_VALUE;
    /**
     * A fixed value representing the batch version.
     */
    private final byte magic;
    /**
     * The metadata of this record batch.
     */
    private final RecordBatchMetaT batchMeta;
    /**
     * The records in this record batch.
     */
    private final List<Record> records;

    public RecordBatch(long streamId, short flags, byte magic, Map<String, String> batchProperties,
        List<Record> records) {
        Preconditions.checkArgument(records != null && records.size() > 0, "records should not be empty");

        this.magic = magic;
        this.records = records;

        long baseOffset = records.get(0).getOffset();
        long baseTimestamp = records.get(0).getTimestamp();
        this.batchMeta = new RecordBatchMetaT();
        batchMeta.setStreamId(streamId);
        batchMeta.setBaseOffset(baseOffset);
        batchMeta.setBaseTimestamp(baseTimestamp);
        batchMeta.setFlags(flags);
        if (batchProperties != null && !batchProperties.isEmpty()) {
            batchMeta.setProperties(ProtocolUtil.map2KeyValueTArray(batchProperties));
        }

        Preconditions.checkArgument(records.get(records.size() - 1).getOffset() - baseOffset <= MAX_OFFSET_DELTA,
            "offset delta should not exceed " + MAX_OFFSET_DELTA);
        batchMeta.setLastOffsetDelta((int) (records.get(records.size() - 1).getOffset() - baseOffset));
    }

    public RecordBatch(long streamId, short flags, Map<String, String> batchProperties, List<Record> records) {
        this(streamId, flags, DEFAULT_MAGIC, batchProperties, records);
    }

    public RecordBatch(long streamId, Map<String, String> batchProperties, List<Record> records) {
        this(streamId, DEFAULT_FLAG, batchProperties, records);
    }

    public RecordBatch(long streamId, List<Record> records) {
        this(streamId, DEFAULT_FLAG, null, records);
    }

    public RecordBatch(ByteBuffer buffer) {
        Preconditions.checkArgument(buffer != null && buffer.remaining() >= MIN_LENGTH, "buffer should contain at least " + MIN_LENGTH + " bytes");
        this.magic = buffer.get();
        long baseOffset = buffer.getLong();
        int metaLength = buffer.getInt();

        assert buffer.remaining() >= metaLength;
        byte[] batchMetaBytes = new byte[metaLength];
        buffer.get(batchMetaBytes);
        this.batchMeta = RecordBatchMeta.getRootAsRecordBatchMeta(ByteBuffer.wrap(batchMetaBytes)).unpack();

        // Clients have to accept the outside BaseOffset.
        this.batchMeta.setBaseOffset(baseOffset);
        this.records = extractRecordList(buffer);
    }

    /**
     * Decode a list of record batches from a byte buffer. Only the first ${count} batches will be decoded.
     * Note that it will decode as many batches as possible if the byte buffer contains less than ${count} batches.
     *
     * @param buffer the byte buffer containing the record batches
     * @param count the count of batches need to be extracted from the byte buffer
     * @return a list of record batches
     */
    public static List<RecordBatch> decode(ByteBuffer buffer, int count) {
        if (count <= 0) {
            return new ArrayList<>();
        }
        List<RecordBatch> batchList = new ArrayList<>(count);
        while (buffer.remaining() >= MIN_LENGTH && batchList.size() < count) {
            batchList.add(new RecordBatch(buffer));
        }
        return batchList;
    }

    /**
     * Encode to a byte buffer.
     *
     * @return a byte buffer, which is ready to be stored or sent.
     */
    public ByteBuffer encode() {
        int totalLength = MIN_LENGTH;
        List<ByteBuffer[]> recordsBufferList = new ArrayList<>(records.size());
        for (Record record : records) {
            RecordMetaT metaT = new RecordMetaT();
            assert record.getTimestamp() - batchMeta.getBaseTimestamp() <= MAX_OFFSET_DELTA && record.getTimestamp() - -batchMeta.getBaseTimestamp() >= Integer.MIN_VALUE;
            metaT.setTimestampDelta((int) (record.getTimestamp() - batchMeta.getBaseTimestamp()));
            if (record.getHeaders() != null) {
                metaT.setHeaders(record.getHeaders().toKeyValueTArray());
            }
            if (record.getProperties() != null && !record.getProperties().isEmpty()) {
                metaT.setProperties(ProtocolUtil.map2KeyValueTArray(record.getProperties()));
            }
            metaT.setOffsetDelta((int) (record.getOffset() - batchMeta.getBaseOffset()));
            FlatBufferBuilder builder = new FlatBufferBuilder();
            int metaOffset = RecordMeta.pack(builder, metaT);
            builder.finish(metaOffset);

            // add meta and body
            recordsBufferList.add(new ByteBuffer[] {builder.dataBuffer(), record.getBody().duplicate()});
            totalLength += Record.ENCODED_MIN_LENGTH + builder.dataBuffer().remaining() + record.getBody().remaining();
        }

        FlatBufferBuilder builder = new FlatBufferBuilder();
        int metaOffset = RecordBatchMeta.pack(builder, batchMeta);
        builder.finish(metaOffset);
        totalLength += builder.dataBuffer().remaining();

        // Note that there are two BaseOffsets when encoding. One is kept in {@link RecordBatchMeta}, and the other is generated when encoding.
        // The data nodes may modify and only modify the BaseOffset outside the {@link RecordBatchMeta} when storing this batch.
        ByteBuffer resultBuffer = ByteBuffer.allocate(totalLength)
            .put(magic)
            .putLong(batchMeta.getBaseOffset())
            .putInt(builder.dataBuffer().remaining())
            .put(builder.dataBuffer());
        for (ByteBuffer[] bufferArray : recordsBufferList) {
            // put in MetaLength
            resultBuffer.putInt(bufferArray[0].remaining());
            // put in BodyLength
            resultBuffer.putInt(bufferArray[1].remaining());
            // put in RecordMeta
            resultBuffer.put(bufferArray[0]);
            // put in RecordBody
            resultBuffer.put(bufferArray[1]);
        }

        resultBuffer.flip();
        return resultBuffer;
    }

    public byte getMagic() {
        return magic;
    }

    public long getBaseOffset() {
        return batchMeta.getBaseOffset();
    }

    public RecordBatchMetaT getBatchMeta() {
        return batchMeta;
    }

    public List<Record> getRecords() {
        return records;
    }

    /**
     * Check if the records in this batch are equivalent to the records in the other batch.
     * Note that the records in the two batches may differ in the offset, but the offset delta should be the same.
     *
     * @param other the other batch
     * @return true if the records in this batch are equivalent to the records in the other batch
     */
    public boolean recordsEquivalent(RecordBatch other) {
        if (other == null) {
            return false;
        }
        if (records.size() != other.records.size()) {
            return false;
        }
        for (int i = 0; i < records.size(); i++) {
            if (!records.get(i).equivalent(other.records.get(i))) {
                return false;
            }
            long offsetDelta = records.get(i).getOffset() - getBaseOffset();
            long otherOffsetDelta = other.records.get(i).getOffset() - other.getBaseOffset();
            if (offsetDelta != otherOffsetDelta) {
                return false;
            }
        }
        return true;
    }


    private List<Record> extractRecordList(ByteBuffer buffer) {
        assert buffer != null;

        int lastDelta = this.batchMeta.getLastOffsetDelta();
        List<Record> recordList = new ArrayList<>();
        while (buffer.remaining() >= Record.ENCODED_MIN_LENGTH) {
            Record record = new Record(buffer, this.batchMeta.getStreamId(), getBaseOffset(), this.batchMeta.getBaseTimestamp());
            recordList.add(record);
            // Meet the ending of this batch.
            if (record.getOffset() - getBaseOffset() >= lastDelta) {
                break;
            }
        }
        return recordList;
    }
}
