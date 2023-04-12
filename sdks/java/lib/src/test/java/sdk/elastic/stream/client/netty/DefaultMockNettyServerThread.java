package sdk.elastic.stream.client.netty;

import java.io.Closeable;
import java.io.IOException;

public class DefaultMockNettyServerThread implements Closeable {
    private final Thread serverThread;
    private final DefaultMockNettyServer nettyServer;

    @Override
    public void close() throws IOException {
        nettyServer.close();
        try {
            Thread.sleep(1000);
        } catch (InterruptedException e) {
            throw new RuntimeException(e);
        }
        serverThread.interrupt();
    }

    public DefaultMockNettyServerThread(int serverPort, String defaultResponsePrefix) {
        nettyServer = new DefaultMockNettyServer(1, serverPort, defaultResponsePrefix);
        serverThread = new Thread(() -> {
            try {
                nettyServer.start();
            } catch (InterruptedException e) {
                throw new RuntimeException(e);
            }
        });
    }

    public void start() throws InterruptedException {
        serverThread.start();
        // make sure server gets off the ground
        Thread.sleep(1000);
    }
}
