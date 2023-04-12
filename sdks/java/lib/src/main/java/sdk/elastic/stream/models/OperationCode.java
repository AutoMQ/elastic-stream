package sdk.elastic.stream.models;

public enum OperationCode {
    PING((short) 0x0001),
    HEARTBEAT((short) 0x0003),
    APPEND((short) 0x1001),
    FETCH((short) 0x1002),
    LIST_RANGES((short) 0x2001),
    SEAL_RANGES((short) 0x2002),
    DESCRIBE_RANGES((short) 0x2004),
    CREATE_STREAMS((short) 0x3001),
    DESCRIBE_STREAMS((short) 0x3004),
    ;
    private final short code;
    OperationCode(short code) {
        this.code = code;
    }
    public short getCode() {
        return code;
    }

}
