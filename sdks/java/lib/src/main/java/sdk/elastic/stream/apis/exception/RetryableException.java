package sdk.elastic.stream.apis.exception;

public class RetryableException extends ClientException {

    private static final long serialVersionUID = 6338133491680948104L;

    public RetryableException(String message, Throwable cause) {
        super(message, cause);
    }

    public RetryableException(String message) {
        super(message);
    }

    public RetryableException(Throwable cause) {
        super(cause);
    }

    public RetryableException() {
        super();
    }
}
