package com.automq.elasticstream.client.jni;

import org.junit.Assert;
import org.junit.Test;

import com.automq.elasticstream.client.utils.BytesUtils;

import java.nio.ByteBuffer;

public class FrontendTest {

    @Test
    public void testAllocateAndDeallocate() {
        ByteBuffer buffer = Frontend.allocateDirect(4096);
        Assert.assertEquals(4096, buffer.capacity());
        Assert.assertTrue(buffer.isDirect());
        Frontend.freeMemory(BytesUtils.getAddress(buffer), buffer.capacity());
    }

    @Test
    public void testBenchAllocate() {
        for(int i = 0; i < 65536; i++) {
            ByteBuffer buffer = Frontend.allocateDirect(4096);
            Assert.assertEquals(4096, buffer.capacity());
            Assert.assertTrue(buffer.isDirect());
            Frontend.freeMemory(BytesUtils.getAddress(buffer), buffer.capacity());
        }
    }
}
