package sdk.elastic.stream.models;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

class RecordBatchTest {

    @Test
    public void testEncodeAndDecode() {
        long streamId = 1L;
        RecordBatch batch = RecordBatchesGenerator.generateOneRecordBatch(streamId);
        ByteBuffer encode = batch.encode();
        List<RecordBatch> decodeList = RecordBatch.decode(encode, encode.remaining());

        Assertions.assertEquals(1, decodeList.size());
        RecordBatch decodedBatch = decodeList.get(0);

        Assertions.assertEquals(batch.getMagic(), decodedBatch.getMagic());

        Assertions.assertEquals(batch.getBatchMeta().getBaseTimestamp(), decodedBatch.getBatchMeta().getBaseTimestamp());
        Assertions.assertEquals(batch.getBaseOffset(), decodedBatch.getBaseOffset());
        Assertions.assertEquals(batch.getBatchMeta().getStreamId(), decodedBatch.getBatchMeta().getStreamId());
        Assertions.assertEquals(batch.getBatchMeta().getFlags(), decodedBatch.getBatchMeta().getFlags());
        Assertions.assertEquals(batch.getBatchMeta().getLastOffsetDelta(), decodedBatch.getBatchMeta().getLastOffsetDelta());

        Assertions.assertEquals(batch.getRecords(), decodedBatch.getRecords());
    }

    @Test
    public void testEncodeAndDecodeWithEmptyHeaderAndProperties() {
        long streamId = 2L;
        long baseOffset = 100L;
        List<Record> recordList = new ArrayList<>(2);
        recordList.add(new Record(streamId, baseOffset + 1, 13L, null, null, ByteBuffer.wrap(new byte[] {0, 1, 3})));
        recordList.add(new Record(streamId, baseOffset + 2, 11L, null, null, ByteBuffer.wrap(new byte[] {3, 6, 8, 10})));
        RecordBatch batch = new RecordBatch(streamId, recordList);

        ByteBuffer encode = batch.encode();
        List<RecordBatch> decodeList = RecordBatch.decode(encode, encode.remaining());

        Assertions.assertEquals(1, decodeList.size());
        RecordBatch decodedBatch = decodeList.get(0);

        Assertions.assertEquals(batch.getMagic(), decodedBatch.getMagic());

        Assertions.assertEquals(batch.getBatchMeta().getBaseTimestamp(), decodedBatch.getBatchMeta().getBaseTimestamp());
        Assertions.assertEquals(batch.getBaseOffset(), decodedBatch.getBaseOffset());
        Assertions.assertEquals(batch.getBatchMeta().getStreamId(), decodedBatch.getBatchMeta().getStreamId());
        Assertions.assertEquals(batch.getBatchMeta().getFlags(), decodedBatch.getBatchMeta().getFlags());
        Assertions.assertEquals(batch.getBatchMeta().getLastOffsetDelta(), decodedBatch.getBatchMeta().getLastOffsetDelta());

        Assertions.assertEquals(batch.getRecords(), decodedBatch.getRecords());
        Assertions.assertEquals(0, decodedBatch.getBatchMeta().getProperties().length);
    }
}