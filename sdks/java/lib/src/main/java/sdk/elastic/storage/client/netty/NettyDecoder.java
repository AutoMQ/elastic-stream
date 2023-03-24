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
import io.netty.channel.ChannelHandlerContext;
import io.netty.handler.codec.LengthFieldBasedFrameDecoder;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

public class NettyDecoder extends LengthFieldBasedFrameDecoder {
    private static final Logger log = LoggerFactory.getLogger(NettyDecoder.class);

    public NettyDecoder() {
        super(Integer.MAX_VALUE, 0, SbpFrame.FRAME_SIZE_LENGTH, 0, 0);
    }

    @Override
    protected Object decode(ChannelHandlerContext ctx, ByteBuf in) throws Exception {
        ByteBuf buf = null;
        try {
            buf = (ByteBuf) super.decode(ctx, in);
            if (null == buf) {
                return null;
            }
            return new SbpFrame(buf.duplicate());
        } catch (Exception e) {
            log.error("decode exception, " + RemotingUtil.parseChannelRemoteAddr(ctx.channel()), e);
            if (null != buf) {
                buf.release();
            }
            RemotingUtil.closeChannel(ctx.channel());
        }

        return null;
    }
}
