package sdk.elastic.stream.models;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static sdk.elastic.stream.models.HeaderKey.CreatedAt;
import static sdk.elastic.stream.models.HeaderKey.Tag;

public class RecordBatchesGenerator {
    public static RecordBatch generateOneRecordBatchWithNoProperties(long streamId) {
        List<Record> recordList = generateRecordList(streamId, 0L);
        return new RecordBatch(streamId, recordList);
    }
    public static RecordBatch generateOneRecordBatchWithNoProperties(long streamId, long baseOffset) {
        List<Record> recordList = generateRecordList(streamId, baseOffset);
        return new RecordBatch(streamId, recordList);
    }

    public static RecordBatch generateOneRecordBatch(long streamId) {
        return generateOneRecordBatch(streamId, 0L);
    }

    public static RecordBatch generateOneRecordBatch(long streamId, long baseOffset) {
        List<Record> recordList = generateRecordList(streamId, baseOffset);
        Map<String, String> batchProperties = new HashMap<>();
        batchProperties.put("batchPropertyA", "batchValueA");
        batchProperties.put("batchPropertyB", "batchValueB");
        return new RecordBatch(streamId, batchProperties, recordList);
    }

    public static List<Record> generateRecordList(long streamId, long baseOffset) {
        List<Record> recordList = new ArrayList<>(2);

        Headers headers = new Headers();
        headers.addHeader(CreatedAt, "someTime");
        Map<String, String> properties = new HashMap<>();
        properties.put("propertyA", "valueA");
        recordList.add(new Record(streamId, baseOffset, 13L, headers, properties, ByteBuffer.wrap(new byte[] {0, 1, 3})));

        Headers headers2 = new Headers();
        headers2.addHeader(CreatedAt, "someTime2");
        headers2.addHeader(Tag, "onlyTest");
        Map<String, String> properties2 = new HashMap<>();
        properties2.put("propertyB", "valueB");
        properties2.put("propertyC", "valueC");
        recordList.add(new Record(streamId, baseOffset + 10, 11L, headers2, properties2, ByteBuffer.wrap(new byte[] {3, 6, 8, 10})));

        return recordList;
    }
}
