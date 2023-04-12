package sdk.elastic.stream.client.cache;

import com.google.common.cache.CacheBuilder;
import com.google.common.cache.CacheLoader;
import com.google.common.cache.LoadingCache;
import java.time.Duration;

/**
 * CommonCache is a basic cache for use.
 * @param <K> key type
 * @param <V> value type
 */
public abstract class CommonCache<K, V> {
    private final LoadingCache<K, V> cache;

    public CommonCache(int size, CacheLoader<K, V> loader) {
        this.cache = CacheBuilder.newBuilder()
            .maximumSize(size)
            .build(loader);
    }

    public CommonCache(int size, Duration expireAccessTime, CacheLoader<K, V> loader) {
        this.cache = CacheBuilder.newBuilder()
            .maximumSize(size)
            .expireAfterAccess(expireAccessTime)
            .build(loader);
    }

    /**
     * Get value by key.
     * Returns the value associated with the key in this cache, obtaining that value from CacheLoader.load(Object) if necessary.
     *
     * @param key with which the specified value is to be associated
     * @return the current (existing or computed) value associated with the specified key, or null if the computed value is null
     */
    public V get(K key) {
        return cache.getUnchecked(key);
    }

    /**
     * Put value by key.
     * If key is null or value is null, do nothing.
     *
     * @param key with which the specified value is to be associated
     * @param value the value to be stored
     */
    public void put(K key, V value) {
        if (key != null && value != null) {
            cache.put(key, value);
        }
    }


    /**
     * Refresh value by key.
     *
     * @param key - with which the specified value is to be associated
     */
    public void refresh(K key) {
        cache.refresh(key);
    }

    /**
     * Invalidate the entry by key.
     *
     * @param key - with which the specified value is to be associated
     */
    public void invalidate(K key) {
        cache.invalidate(key);
    }

    /**
     * Returns the approximate number of entries in this cache. The value returned is an estimate.
     *
     * @return the estimated size of this cache.
     */
    public long getSize() {
        return cache.size();
    }
}
