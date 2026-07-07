use std::sync::OnceLock;

use arch_db::{command::Command, engine::{Engine, Value}, sstable_manager::init_sstable_counter, storage_iterator::StorageIterator};



pub fn ensure_counter_initialized() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        init_sstable_counter();
    });
}

#[test]
fn test_engine_iterator_memtable() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    engine.execute(Command::Set("a".to_string(), "1".to_string()));
    engine.execute(Command::Set("b".to_string(), "2".to_string()));
    engine.execute(Command::Set("c".to_string(), "3".to_string()));

    let mut iter = engine.iter().unwrap();

    let r = iter.next().unwrap().unwrap();
    println!("key: {:?}", r);
    assert_eq!(r.key, "a");
    assert_eq!(r.value, Value::Data("1".to_string()));

    let r = iter.next().unwrap().unwrap();
    assert_eq!(r.key, "b");
    assert_eq!(r.value, Value::Data("2".to_string()));

    let r = iter.next().unwrap().unwrap();
    assert_eq!(r.key, "c");
    assert_eq!(r.value, Value::Data("3".to_string()));

    assert!(iter.next().unwrap().is_none());
}

#[test]
fn test_engine_iterator_empty() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    let mut iter = engine.iter().unwrap();

    assert!(iter.next().unwrap().is_none());
}