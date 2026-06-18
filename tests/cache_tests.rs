use arch_db::cache::{BlockCache, CacheKey};
use arch_db::engine::Value;
use arch_db::sstable::BlockRecord;

#[test]
fn test_cache_insert_get() {
    let mut cache = BlockCache::new(2);

    let key = CacheKey {
        path: "a".into(),
        offset: 0,
    };

    cache.insert(
        key.clone(),
        vec![BlockRecord {key: "x".into(), value: Value::Data("1".into())}],
    );

    let result = cache.get(&key);
    assert!(result.is_some());
}

#[test]
fn test_lru_eviction() {
    let mut cache = BlockCache::new(2);

    let k1 = CacheKey { path: "a".into(), offset: 1 };
    let k2 = CacheKey { path: "b".into(), offset: 2 };
    let k3 = CacheKey { path: "c".into(), offset: 3 };

    cache.insert(k1.clone(), vec![]);
    cache.insert(k2.clone(), vec![]);

    // Access k1 to make it most recently used
    cache.get(&k1);

    // Insert k3 — should evict k2 (LRU)
    cache.insert(k3.clone(), vec![]);

    assert!(cache.get(&k1).is_some());
    assert!(cache.get(&k2).is_none());
    assert!(cache.get(&k3).is_some());
}

#[test]
fn test_cache_recency_update() {
    let mut cache = BlockCache::new(2);

    let k1 = CacheKey { path: "a".into(), offset: 1 };
    let k2 = CacheKey { path: "b".into(), offset: 2 };

    cache.insert(k1.clone(), vec![]);
    cache.insert(k2.clone(), vec![]);

    // Before get, k2 is the most recent (inserted last)
    // After get, k1 becomes the most recent
    cache.get(&k1);

    // Insert a third key — should evict the LRU (which is now k2)
    let k3 = CacheKey { path: "c".into(), offset: 3 };
    cache.insert(k3.clone(), vec![]);

    assert!(cache.get(&k1).is_some(), "k1 was recently accessed, should survive");
    assert!(cache.get(&k2).is_none(), "k2 was LRU, should be evicted");
    assert!(cache.get(&k3).is_some(), "k3 was just inserted, should be present");
}