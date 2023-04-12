/*
 * Licensed to the Apache Software Foundation (ASF) under one or more
 * contributor license agreements.  See the NOTICE file distributed with
 * this work for additional information regarding copyright ownership.
 * The ASF licenses this file to You under the Apache License, Version 2.0
 * (the "License"); you may not use this file except in compliance with
 * the License.  You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package sdk.elastic.stream.client.netty;

import io.netty.channel.Channel;
import java.util.concurrent.CompletableFuture;
import sdk.elastic.stream.client.protocol.SbpFrame;

public class ResponseFuture {
    private final int id;
    private final Channel channel;
    private final SbpFrame request;
    private final long timeoutMillis;
    private final CompletableFuture<SbpFrame> completableFuture;
    private final long beginTimestamp = System.currentTimeMillis();
    private volatile boolean sendRequestOK = true;

    public ResponseFuture(Channel channel, int id, long timeoutMillis, CompletableFuture<SbpFrame> completableFuture) {
        this(channel, id, null, timeoutMillis, completableFuture);
    }

    public ResponseFuture(Channel channel, int id, SbpFrame request, long timeoutMillis, CompletableFuture<SbpFrame> completableFuture) {
        this.id = id;
        this.channel = channel;
        this.request = request;
        this.timeoutMillis = timeoutMillis;
        this.completableFuture = completableFuture;
    }

    public boolean isTimeout() {
        long diff = System.currentTimeMillis() - this.beginTimestamp;
        return diff > this.timeoutMillis;
    }

    public long getBeginTimestamp() {
        return beginTimestamp;
    }

    public boolean isSendRequestOK() {
        return sendRequestOK;
    }

    public void setSendRequestOK(boolean sendRequestOK) {
        this.sendRequestOK = sendRequestOK;
    }

    public long getTimeoutMillis() {
        return timeoutMillis;
    }

    public CompletableFuture<SbpFrame> getCompletableFuture() {
        return completableFuture;
    }

    public SbpFrame getRequestSbpFrame() {
        return request;
    }

    public Channel getChannel() {
        return channel;
    }
}
