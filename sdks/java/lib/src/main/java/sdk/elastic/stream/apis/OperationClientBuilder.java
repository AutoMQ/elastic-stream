package sdk.elastic.stream.apis;

public interface OperationClientBuilder {
    /**
     * Set client configuration.
     *
     * @param clientConfiguration client configuration.
     * @return OperationClientBuilder instance.
     */
    OperationClientBuilder setClientConfiguration(ClientConfiguration clientConfiguration);
    /**
     * Build OperationClient.
     *
     * @return OperationClient instance.
     * @throws Exception throws if building OperationClient failed.
     */
    OperationClient build() throws Exception;
}
