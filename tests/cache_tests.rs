#[cfg(test)]
// Basic Insert/Get

#[test]
fn test_cache_insert_get() {

    let mut cache =
        BlockCache::new(2);

    let key = CacheKey {

        path: "a".into(),

        offset: 0,
    };

    cache.insert(
        key.clone(),

        vec![
            (
                "x".into(),
                Value::Data(
                    "1".into()
                )
            )
        ],
    );

    let result =
        cache.get(&key);

    assert!(result.is_some());
}


#[test]
// LRU Eviction
fn test_lru_eviction() {

    let mut cache =
        BlockCache::new(2);

    let k1 = CacheKey {
        path: "a".into(),
        offset: 1,
    };

    let k2 = CacheKey {
        path: "b".into(),
        offset: 2,
    };

    let k3 = CacheKey {
        path: "c".into(),
        offset: 3,
    };

    cache.insert(
        k1.clone(),
        vec![],
    );

    cache.insert(
        k2.clone(),
        vec![],
    );

    cache.get(&k1);

    cache.insert(
        k3.clone(),
        vec![],
    );

    assert!(
        cache.get(&k1)
            .is_some()
    );

    assert!(
        cache.get(&k2)
            .is_none()
    );
}


#[test]
// Recency Update
fn test_cache_recency_update() {

    let mut cache =
        BlockCache::new(2);

    let k1 = CacheKey {
        path: "a".into(),
        offset: 1,
    };

    let k2 = CacheKey {
        path: "b".into(),
        offset: 2,
    };

    cache.insert(
        k1.clone(),
        vec![],
    );

    cache.insert(
        k2.clone(),
        vec![],
    );

    cache.get(&k1);

    assert_eq!(
        cache.usage.back(),
        Some(&k1)
    );
}