package models;

public enum OperationCode {
    APPEND((short) 0x1001),
    FETCH((short) 0x1002),
    LIST_RANGES((short) 0x2001),
    SEAL_RANGES((short) 0x2002),
    DESCRIBE_RANGES((short) 0x2004),
    CREATE_STREAMS((short) 0x3001),
    ;
    private final short code;
    OperationCode(short code) {
        this.code = code;
    }
    public short getCode() {
        return code;
    }

}
