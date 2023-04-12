package sdk.elastic.stream.client.cache;

import com.google.common.cache.CacheLoader;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

class CommonCacheTest {
    private static CacheLoader<Long, String> emptyCacheLoader = new CacheLoader<Long, String>() {
        @Override
        public String load(Long key) throws Exception {
            return "";
        }
    };

    @Test
    void get() {
        CommonCacheTestImpl cache = new CommonCacheTestImpl();
        assertEquals(0, cache.get(0L).length());
    }

    @Test
    void getError() {
        CommonCacheTestImpl cache = new CommonCacheTestImpl(new CacheLoader<Long, String>() {
            @Override
            public String load(Long key) throws Exception {
                return null;
            }
        });
        assertThrows(RuntimeException.class, () -> cache.get(0L));
    }

    @Test
    void put() {
        CommonCacheTestImpl cache = new CommonCacheTestImpl();
        long key = 2L;
        String value = "test";
        cache.put(key, value);
        assertEquals(value, cache.get(key));
    }

    @Test
    void refresh() {
        long key = 3L;
        String value = "test";
        CommonCacheTestImpl cache = new CommonCacheTestImpl();
        cache.put(key, value);
        assertEquals(value, cache.get(key));

        cache.refresh(key);
        assertEquals(0, cache.get(key).length());
    }

    @Test
    void invalidate() {
        long key = 4L;
        String value = "test";
        CommonCacheTestImpl cache = new CommonCacheTestImpl();
        cache.put(key, value);
        assertEquals(1, cache.getSize());

        cache.invalidate(key);
        assertEquals(0, cache.getSize());
    }

    @Test
    void getSize() {
        CommonCacheTestImpl cache = new CommonCacheTestImpl();
        cache.get(0L);
        cache.get(1L);
        assertEquals(2, cache.getSize());
    }

    private static class CommonCacheTestImpl extends CommonCache<Long, String> {
        private static final int DEFAULT_CACHE_SIZE = 100;

        public CommonCacheTestImpl(CacheLoader<Long, String> cacheLoader) {
            super(DEFAULT_CACHE_SIZE, cacheLoader);
        }

        public CommonCacheTestImpl() {
            this(emptyCacheLoader);
        }
    }
}