package sdk.elastic.stream.client.common;

import java.nio.ByteBuffer;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;
import sdk.elastic.stream.client.protocol.SbpFrame;
import sdk.elastic.stream.client.protocol.SbpFrameBuilder;
import sdk.elastic.stream.flatc.records.KeyValueT;
import sdk.elastic.stream.models.HeaderKey;
import sdk.elastic.stream.models.OperationCode;

public class ProtocolUtil {
    /**
     * Construct a request SbpFrame
     *
     * @param operationCode operation code
     * @param header        header
     * @return generated SbpFrame
     */
    public static SbpFrame constructRequestSbpFrame(OperationCode operationCode, ByteBuffer header) {
        return constructRequestSbpFrame(operationCode, header, null);
    }

    /**
     * Construct a request SbpFrame
     *
     * @param operationCode operation code
     * @param header        header
     * @param payloads      payloads, can be null
     * @return generated SbpFrame
     */
    public static SbpFrame constructRequestSbpFrame(OperationCode operationCode, ByteBuffer header,
        ByteBuffer[] payloads) {
        return new SbpFrameBuilder()
            .setFlag(SbpFrame.DEFAULT_REQUEST_FLAG)
            .setOperationCode(operationCode.getCode())
            .setHeader(header)
            .setPayload(payloads)
            .build();
    }

    public static Map<String, String> keyValueTList2Map(List<KeyValueT> list) {
        if (list == null || list.isEmpty()) {
            return new HashMap<>();
        }
        return list.stream()
            .collect(Collectors.toMap(KeyValueT::getKey, KeyValueT::getValue));
    }

    public static Map<String, String> keyValueTArray2Map(KeyValueT[] keyValueTArray) {
        if (keyValueTArray == null || keyValueTArray.length == 0){
            return new HashMap<>();
        }
        return keyValueTList2Map(Arrays.asList(keyValueTArray));
    }

    public static KeyValueT[] map2KeyValueTArray(Map<String, String> map) {
        return map.entrySet()
            .stream()
            .map(entry -> {
                KeyValueT item = new KeyValueT();
                item.setKey(entry.getKey());
                item.setValue(entry.getValue());
                return item;
            }).toArray(KeyValueT[]::new);
    }

    public static Map<HeaderKey, String> keyValueTList2HeaderMap(List<KeyValueT> list) {
        return list.stream()
            .collect(Collectors.toMap(item -> HeaderKey.valueOf(item.getKey()), KeyValueT::getValue));
    }
}
