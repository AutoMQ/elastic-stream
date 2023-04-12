package sdk.elastic.stream.client;

import java.io.Closeable;
import java.time.Duration;
import java.util.concurrent.CompletableFuture;
import sdk.elastic.stream.client.common.ClientId;
import sdk.elastic.stream.client.protocol.RemotingItem;
import sdk.elastic.stream.client.protocol.SbpFrame;
import sdk.elastic.stream.client.route.Address;

public interface RemotingClient extends Closeable {

    /**
     * Get Unique Client Identifier
     *
     * <p>Get the unique client identifier for each client.
     *
     * @return a unique client identifier.
     */
    public ClientId getClientId();

    /**
     * Start a client.
     *
     * @throws Exception
     */
    void start() throws Exception;

    /**
     * Invokes a request asynchronously.
     *
     * @param address address of the remote endpoint.
     * @param request request to be sent.
     * @param timeout timeout for the request.
     * @return the response.
     */
    CompletableFuture<SbpFrame> invokeAsync(Address address, RemotingItem request, Duration timeout);
}
