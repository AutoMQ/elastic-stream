package sdk.elastic.stream.apis.exception;

public class SealableException extends ClientException {
    private static final long serialVersionUID = 1L;

    public SealableException() {
        super();
    }

    public SealableException(String message) {
        super(message);
    }

    public SealableException(String message, Throwable cause) {
        super(message, cause);
    }

    public SealableException(Throwable cause) {
        super(cause);
    }

}
