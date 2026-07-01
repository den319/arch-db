use std::collections::BTreeMap;

use arch_db::{engine::Value, memtable_iterator::MemtableIterator, storage_iterator::StorageIterator};

#[test]
fn test_memtable_iterator() {
    let mut memtable = BTreeMap::new();
    memtable.insert("a".to_string(), Value::Data("1".to_string()));
    memtable.insert("b".to_string(), Value::Data("2".to_string()));

    let mut iter = MemtableIterator::new(&memtable);

    assert_eq!(
        iter.next().unwrap().unwrap().key,
        "a"
    );

    assert_eq!(
        iter.next().unwrap().unwrap().key,
        "b"
    );

    assert!(
        iter.next().unwrap().is_none()
    );
}
