package sdk.elastic.stream.client.impl;

import com.google.common.base.Preconditions;
import sdk.elastic.stream.apis.ClientConfiguration;
import sdk.elastic.stream.apis.OperationClient;
import sdk.elastic.stream.apis.OperationClientBuilder;

public class OperationClientBuilderImpl implements OperationClientBuilder {
    private ClientConfiguration clientConfiguration;
    @Override
    public OperationClientBuilder setClientConfiguration(ClientConfiguration clientConfiguration) {
        Preconditions.checkArgument(clientConfiguration != null, "clientConfiguration is null");
        this.clientConfiguration = clientConfiguration;
        return this;
    }

    @Override
    public OperationClient build() throws Exception {
        return new OperationClientImpl(clientConfiguration);
    }
}
