package sdk.elastic.stream.client.protocol;

import io.netty.util.ReferenceCounted;

public interface RemotingItem extends ReferenceCounted {
    /**
     * Is this instance a request.
     * @return True if this instance is a request.
     */
    boolean isRequest();
    /**
     * Get Identifier of this instance.
     * @return Identifier of this instance.
     */
    int getId();
}
