use std::sync::OnceLock;

use arch_db::{command::Command, engine::{Engine, Value}, sstable_manager::init_sstable_counter, storage_iterator::StorageIterator};


pub fn ensure_counter_initialized() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        init_sstable_counter();
    });
}

/// Clean up all persistent state from previous test runs.
/// Tests must NOT run in parallel since they share storage paths.
fn clean_all_state() {
    // Remove WAL storage directory entirely
    let path = std::path::Path::new("storage/temp");
    if path.exists() {
        std::fs::remove_dir_all(path).ok();
    }
    // Remove stale SSTable files
    for entry in std::fs::read_dir(".").ok().into_iter().flatten() {
        if let Ok(entry) = entry {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("sst_") && name.ends_with(".bin") {
                std::fs::remove_file(entry.path()).ok();
            }
        }
    }
    // Remove manifest files
    std::fs::remove_file("MANIFEST.log").ok();
    std::fs::remove_file("MANIFEST.checkpoint").ok();
}

#[test]
fn test_engine_iterator_memtable() {
    ensure_counter_initialized();
    clean_all_state();

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
    clean_all_state();

    let mut engine = Engine::new();

    let mut iter = engine.iter().unwrap();

    assert!(iter.next().unwrap().is_none());
}