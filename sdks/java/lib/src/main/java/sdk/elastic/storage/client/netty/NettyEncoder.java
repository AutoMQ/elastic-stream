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
package sdk.elastic.storage.client.netty;

import sdk.elastic.storage.client.common.RemotingUtil;
import sdk.elastic.storage.client.protocol.SbpFrame;
import io.netty.buffer.ByteBuf;
import io.netty.channel.ChannelHandler;
import io.netty.channel.ChannelHandlerContext;
import io.netty.handler.codec.MessageToByteEncoder;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

@ChannelHandler.Sharable
public class NettyEncoder extends MessageToByteEncoder<SbpFrame> {
    private static final Logger log = LoggerFactory.getLogger(NettyEncoder.class);

    @Override
    protected void encode(ChannelHandlerContext ctx, SbpFrame frame, ByteBuf out)
        throws Exception {
        try {
            byte[] encodeBytes = frame.encode();
            if (encodeBytes != null) {
                out.writeBytes(encodeBytes);
            }
        } catch (Exception e) {
            log.error("encode exception, " + RemotingUtil.parseChannelRemoteAddr(ctx.channel()), e);
            if (frame != null) {
                log.error(frame.toString());
            }
            RemotingUtil.closeChannel(ctx.channel());
        }
    }
}
