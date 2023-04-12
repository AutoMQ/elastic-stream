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
package sdk.elastic.stream.client.common;

import io.netty.channel.Channel;
import io.netty.channel.ChannelFutureListener;
import io.netty.util.Attribute;
import io.netty.util.AttributeKey;
import java.net.InetSocketAddress;
import java.net.SocketAddress;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import sdk.elastic.stream.client.route.Address;

public class RemotingUtil {
    public static final String DEFAULT_CHARSET = "UTF-8";
    public static final String DEFAULT_CIDR_ALL = "0.0.0.0/0";

    private static final Logger log = LoggerFactory.getLogger(RemotingUtil.class);
    private static final AttributeKey<String> REMOTE_ADDR_KEY = AttributeKey.valueOf("RemoteAddr");

    public static SocketAddress string2SocketAddress(final String addr) {
        int split = addr.lastIndexOf(":");
        String host = addr.substring(0, split);
        String port = addr.substring(split + 1);
        InetSocketAddress isa = new InetSocketAddress(host, Integer.parseInt(port));
        return isa;
    }

    public static String parseChannelRemoteAddr(final Channel channel) {
        if (null == channel) {
            return "";
        }
        Attribute<String> att = channel.attr(REMOTE_ADDR_KEY);
        if (att == null) {
            // mocked in unit test
            return parseChannelRemoteAddr0(channel);
        }
        String addr = att.get();
        if (addr == null) {
            addr = parseChannelRemoteAddr0(channel);
            att.set(addr);
        }
        return addr;
    }

    public static Address parseChannelRemoteAddress(final Channel channel) {
        String addr = parseChannelRemoteAddr(channel);
        if (addr.length() == 0) {
            return null;
        }
        String[] addrArr = addr.split(":");
        if (addrArr.length == 2) {
            return new Address(addrArr[0], Integer.parseInt(addrArr[1]));
        }
        return null;
    }

    private static String parseChannelRemoteAddr0(final Channel channel) {
        SocketAddress remote = channel.remoteAddress();
        final String addr = remote != null ? remote.toString() : "";

        if (addr.length() > 0) {
            int index = addr.lastIndexOf("/");
            if (index >= 0) {
                return addr.substring(index + 1);
            }

            return addr;
        }

        return "";
    }

    public static String parseHostFromAddress(String address) {
        if (address == null) {
            return "";
        }

        String[] addressSplits = address.split(":");
        if (addressSplits.length < 1) {
            return "";
        }

        return addressSplits[0];
    }

    public static String parseSocketAddressAddr(SocketAddress socketAddress) {
        if (socketAddress != null) {
            // Default toString of InetSocketAddress is "hostName/IP:port"
            final String addr = socketAddress.toString();
            int index = addr.lastIndexOf("/");
            return (index != -1) ? addr.substring(index + 1) : addr;
        }
        return "";
    }

    public static int parseSocketAddressPort(SocketAddress socketAddress) {
        if (socketAddress instanceof InetSocketAddress) {
            return ((InetSocketAddress) socketAddress).getPort();
        }
        return -1;
    }

    public static int ipToInt(String ip) {
        String[] ips = ip.split("\\.");
        return (Integer.parseInt(ips[0]) << 24)
            | (Integer.parseInt(ips[1]) << 16)
            | (Integer.parseInt(ips[2]) << 8)
            | Integer.parseInt(ips[3]);
    }

    public static boolean ipInCIDR(String ip, String cidr) {
        int ipAddr = ipToInt(ip);
        String[] cidrArr = cidr.split("/");
        int netId = Integer.parseInt(cidrArr[1]);
        int mask = 0xFFFFFFFF << (32 - netId);
        int cidrIpAddr = ipToInt(cidrArr[0]);

        return (ipAddr & mask) == (cidrIpAddr & mask);
    }

    public static void closeChannel(Channel channel) {
        final String addressRemote = parseChannelRemoteAddr(channel);
        if ("".equals(addressRemote)) {
            channel.close();
        } else {
            channel.close().addListener((ChannelFutureListener) future -> log.info("closeChannel: close the connection to remote address[{}] result: {}", addressRemote,
                future.isSuccess()));
        }
    }
}
