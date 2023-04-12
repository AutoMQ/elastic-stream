package sdk.elastic.stream.models;

import java.util.HashMap;
import java.util.Map;
import sdk.elastic.stream.flatc.records.KeyValueT;

public class Headers {
    private Map<HeaderKey, String> headers;

    public Headers() {
        this.headers = new HashMap<>();
    }

    public void addHeader(HeaderKey key, String value) {
        this.headers.put(key, value);
    }

    public String getHeader(HeaderKey key) {
        return this.headers.get(key);
    }
    public KeyValueT[] toKeyValueTArray() {
        KeyValueT[] keyValueTArray = new KeyValueT[this.headers.size()];
        int i = 0;
        for (Map.Entry<HeaderKey, String> entry : this.headers.entrySet()) {
            KeyValueT keyValueT = new KeyValueT();
            keyValueT.setKey(entry.getKey().name());
            keyValueT.setValue(entry.getValue());
            keyValueTArray[i] = keyValueT;
            i++;
        }
        return keyValueTArray;
    }

    @Override
    public boolean equals(Object obj) {
        if (obj == null) {
            return false;
        }
        if (obj == this) {
            return true;
        }
        if (obj.getClass() != getClass()) {
            return false;
        }
        Headers headers = (Headers) obj;
        return this.headers.equals(headers.headers);
    }
}

