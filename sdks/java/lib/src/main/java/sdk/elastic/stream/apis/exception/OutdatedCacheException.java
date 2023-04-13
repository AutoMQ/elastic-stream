package sdk.elastic.stream.apis.exception;

public class OutdatedCacheException extends ClientException {

    private static final long serialVersionUID = 6338133491680948104L;

    public OutdatedCacheException(String message, Throwable cause) {
        super(message, cause);
    }

    public OutdatedCacheException(String message) {
        super(message);
    }

    public OutdatedCacheException(Throwable cause) {
        super(cause);
    }

    public OutdatedCacheException() {
        super();
    }
}
