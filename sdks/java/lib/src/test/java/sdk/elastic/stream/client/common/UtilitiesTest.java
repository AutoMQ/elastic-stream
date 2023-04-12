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

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.NoSuchAlgorithmException;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

public class UtilitiesTest {
    private final String body = "foobar";

    @Test
    public void testCompressAndUncompressByteArray() throws IOException {
        final byte[] bytes = body.getBytes(StandardCharsets.UTF_8);
        final byte[] compressedBytes = Utilities.compressBytesGzip(bytes, 5);
        final byte[] originalBytes = Utilities.uncompressBytesGzip(compressedBytes);
        Assertions.assertEquals(new String(originalBytes, StandardCharsets.UTF_8), body);
    }

    @Test
    public void testCrc32Calculation() {
        Assertions.assertEquals(0, Utilities.crc32Calculation(new ByteBuffer[0]));

        ByteBuffer[] buffers = new ByteBuffer[] {
            ByteBuffer.wrap("foo".getBytes(StandardCharsets.UTF_8)),
            ByteBuffer.wrap("bar".getBytes(StandardCharsets.UTF_8))
        };
        Assertions.assertEquals(2666930069L, Utilities.crc32Calculation(buffers));
    }

    @Test
    public void testCalculateValidBytes() {
        Assertions.assertEquals(0, Utilities.calculateValidBytes(new ByteBuffer[0]));

        ByteBuffer[] buffers = new ByteBuffer[] {
            ByteBuffer.wrap("foo".getBytes(StandardCharsets.UTF_8)),
            ByteBuffer.wrap("bar".getBytes(StandardCharsets.UTF_8))
        };
        Assertions.assertEquals(6, Utilities.calculateValidBytes(buffers));
    }

    @Test
    public void testCrc32CheckSum() {
        final byte[] bytes = body.getBytes(StandardCharsets.UTF_8);
        Assertions.assertEquals("9EF61F95", Utilities.crc32CheckSum(bytes));
    }

    @Test
    public void testMd5CheckSum() throws NoSuchAlgorithmException {
        final byte[] bytes = body.getBytes(StandardCharsets.UTF_8);
        Assertions.assertEquals("3858F62230AC3C915F300C664312C63F", Utilities.md5CheckSum(bytes));
    }

    @Test
    public void testSha1CheckSum() throws NoSuchAlgorithmException {
        final byte[] bytes = body.getBytes(StandardCharsets.UTF_8);
        Assertions.assertEquals("8843D7F92416211DE9EBB963FF4CE28125932878", Utilities.sha1CheckSum(bytes));
    }

    @Test
    public void testStackTrace() {
        final String stackTrace = Utilities.stackTrace();
        Assertions.assertNotNull(stackTrace);
        Assertions.assertTrue(stackTrace.length() > 0);
    }
}