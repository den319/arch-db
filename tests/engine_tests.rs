use std::sync::OnceLock;
use std::fs;

use arch_db::bloom_filter::BloomFilter;

use arch_db::command::Command;
use arch_db::engine::{Engine, Value};
use arch_db::sstable_manager::{init_sstable_counter, SSTable, Level};
use arch_db::helper::unique_file;

/// Initialize the global SSTABLE_COUNTER exactly once across all parallel tests,
/// ensuring no test creates files that collide with each other or with user data.
pub fn ensure_counter_initialized() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        init_sstable_counter();
    });
}

#[test]
fn test_auto_flush_when_memtable_limit_reached() {
    ensure_counter_initialized();

    let mut engine = Engine::new();
    engine.memtable_limit = 2;

    engine.execute(Command::Set("a".to_string(), "1".to_string()));

    assert_eq!(engine.memtable.len(), 1);
    {
        let sstables = engine.sstables.lock().unwrap();
        assert_eq!(sstables.l0.len(), 0);
    }

    engine.execute(Command::Set("b".to_string(), "2".to_string()));

    let path_to_clean = {
        let sstables1 = engine.sstables.lock().unwrap();
        assert_eq!(engine.memtable.len(), 0);
        assert_eq!(sstables1.l0.len(), 1);
        sstables1.l0[0].path.clone()
    };

    match engine.get_key("a") {
        Some(Value::Data(v)) => assert_eq!(v, "1"),
        _ => panic!("expected value 1"),
    }
    match engine.get_key("b") {
        Some(Value::Data(v)) => assert_eq!(v, "2"),
        _ => panic!("expected value 2"),
    }

    fs::remove_file(&path_to_clean).unwrap();
}

#[test]
fn test_auto_flush_preserves_tombstones() {
    ensure_counter_initialized();

    let mut engine = Engine::new();
    engine.memtable_limit = 2;

    engine.execute(Command::Set("user".to_string(), "john".to_string()));
    engine.execute(Command::Del("user".to_string()));
    engine.execute(Command::Set("another".to_string(), "x".to_string()));

    let path_to_clean = {
        let sstables = engine.sstables.lock().unwrap();
        assert_eq!(sstables.l0.len(), 1);
        sstables.l0[0].path.clone()
    };

    match engine.get_key("user") {
        Some(Value::Tombstone) => {}
        _ => panic!("expected tombstone"),
    }

    fs::remove_file(&path_to_clean).unwrap();
}

#[test]
fn test_auto_l0_compaction_trigger() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    for i in 0..12 {
        engine.execute(Command::Set(format!("key{}", i), format!("value{}", i)));

        let file = unique_file("auto_l0_compaction", "bin");
        engine.flush_to_sstable(&file).unwrap();
    }

    let sstables = engine.sstables.lock().unwrap();

    assert!(sstables.l0.len() < 4);
    assert!(!sstables.l1.is_empty());

    let all_paths: Vec<String> = sstables.l0.iter()
        .chain(sstables.l1.iter())
        .chain(sstables.l2.iter())
        .map(|t| t.path.clone())
        .collect();

    for path in all_paths {
        let _ = fs::remove_file(&path);
    }
}

#[test]
fn test_shared_sstable_manager_access() {
    ensure_counter_initialized();

    let engine = Engine::new();

    {
        let sstables = engine.sstables.lock().unwrap();
        assert_eq!(sstables.l0.len(), 0);
        assert_eq!(sstables.l1.len(), 0);
        assert_eq!(sstables.l2.len(), 0);
    }
}

#[test]
fn test_flush_with_shared_sstable_manager() {
    ensure_counter_initialized();

    let mut engine = Engine::new();
    engine.memtable_limit = 2;

    engine.execute(Command::Set("a".to_string(), "1".to_string()));
    engine.execute(Command::Set("b".to_string(), "2".to_string()));

    {
        let sstables = engine.sstables.lock().unwrap();
        assert_eq!(sstables.l0.len(), 1);
    }

    match engine.get_key("a") {
        Some(Value::Data(v)) => assert_eq!(v, "1"),
        _ => panic!("expected value"),
    }

    let paths: Vec<String> = {
        let sstables = engine.sstables.lock().unwrap();
        sstables.l0.iter().map(|t| t.path.clone()).collect()
    };

    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn test_arc_shares_same_sstable_manager() {
    ensure_counter_initialized();

    let engine = Engine::new();

    let shared1 = engine.sstables.clone();
    let shared2 = engine.sstables.clone();

    {
        let mut s1 = shared1.lock().unwrap();
        s1.l0.push(SSTable {
            path: "dummy.bin".to_string(),
            index: arch_db::sstable::SSTableIndex {
                offsets: Default::default(),
                blocks: vec![],
            },
            bloom: BloomFilter::with_rate(0.01, 8),
            level: Level::L0,
            min_key: "a".to_string(),
            max_key: "z".to_string(),
            file_size: 0,
        });
    }

    {
        let s2 = shared2.lock().unwrap();
        assert_eq!(s2.l0.len(), 1);
    }
}

#[test]
fn test_multiple_lock_scopes() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    engine.execute(Command::Set("name".to_string(), "jhon".to_string()));

    {
        let sstables = engine.sstables.lock().unwrap();
        let _ = sstables.l0.len();
    }
    {
        let sstables = engine.sstables.lock().unwrap();
        let _ = sstables.l1.len();
    }

    match engine.get_key("name") {
        Some(Value::Data(v)) => assert_eq!(v, "jhon"),
        _ => panic!("expected value"),
    }
}

#[test]
fn test_compaction_with_shared_manager() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    for i in 0..12 {
        engine.execute(Command::Set(format!("key{}", i), format!("value{}", i)));

        let file = unique_file("shared_compaction", "bin");
        engine.flush_to_sstable(&file).unwrap();
    }

    {
        let sstables = engine.sstables.lock().unwrap();
        assert!(sstables.l0.len() < 4);
        assert!(!sstables.l1.is_empty());
    }

    let paths: Vec<String> = {
        let sstables = engine.sstables.lock().unwrap();
        sstables.l0.iter()
            .chain(sstables.l1.iter())
            .chain(sstables.l2.iter())
            .map(|t| t.path.clone())
            .collect()
    };

    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[test]
// worker thread runs
// compaction executes
// async notification works
fn test_background_compaction_trigger() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    for i in 0..12 {
        engine.execute(Command::Set(format!("key{}", i), format!("value{}", i)));

        let file = unique_file("background_trigger", "bin");
        engine.flush_to_sstable(&file).unwrap();
    }

    std::thread::sleep(std::time::Duration::from_millis(500));

    {
        let sstables = engine.sstables.lock().unwrap();
        assert!(!sstables.l1.is_empty());
    }

    let paths: Vec<String> = {
        let sstables = engine.sstables.lock().unwrap();
        sstables.l0.iter()
            .chain(sstables.l1.iter())
            .chain(sstables.l2.iter())
            .map(|t| t.path.clone())
            .collect()
    };

    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[test]
// writes continue
// engine not blocked
// background compaction isolated
fn test_writes_continue_during_background_compaction() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    for i in 0..20 {
        engine.execute(Command::Set(format!("user{}", i), format!("value{}", i)));

        let file = unique_file("non_blocking", "bin");
        engine.flush_to_sstable(&file).unwrap();
    }

    engine.execute(Command::Set("live_write".to_string(), "works".to_string()));

    match engine.get_key("live_write") {
        Some(Value::Data(v)) => assert_eq!(v, "works"),
        _ => panic!("expected value"),
    }

    let paths: Vec<String> = {
        let sstables = engine.sstables.lock().unwrap();
        sstables.l0.iter()
            .chain(sstables.l1.iter())
            .chain(sstables.l2.iter())
            .map(|t| t.path.clone())
            .collect()
    };

    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[test]
// worker survives repeated notifications
// channel stable
// no panic
// no deadlock
fn test_multiple_background_compaction_signals() {
    ensure_counter_initialized();

    let engine = Engine::new();

    for _ in 0..10 {
        let _ = engine.compaction_tx.send(());
    }

    std::thread::sleep(std::time::Duration::from_millis(200));
}

#[test]
fn test_put_stores_value() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    engine
        .put("name".to_string(), "john".to_string())
        .unwrap();

    match engine.get("name") {
        Some(Value::Data(v)) => assert_eq!(v, "john"),
        _ => panic!("expected value"),
    }
}

#[test]
fn test_get_missing_key_returns_tombstone() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    match engine.get("missing") {
        Some(Value::Tombstone) => {}
        _ => panic!("expected tombstone"),
    }
}

#[test]
fn test_delete_marks_key_as_tombstone() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    engine
        .put("user".to_string(), "alice".to_string())
        .unwrap();

    engine
        .delete("user".to_string())
        .unwrap();

    match engine.get("user") {
        Some(Value::Tombstone) => {}
        _ => panic!("expected tombstone"),
    }
}

#[test]
fn test_delete_nonexistent_key_creates_tombstone() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    engine
        .delete("ghost".to_string())
        .unwrap();

    match engine.get("ghost") {
        Some(Value::Tombstone) => {}
        _ => panic!("expected tombstone"),
    }
}

#[test]
fn test_put_overwrites_existing_value() {
    ensure_counter_initialized();

    let mut engine = Engine::new();

    engine
        .put("user".to_string(), "alice".to_string())
        .unwrap();

    engine
        .put("user".to_string(), "bob".to_string())
        .unwrap();

    match engine.get("user") {
        Some(Value::Data(v)) => assert_eq!(v, "bob"),
        _ => panic!("expected updated value"),
    }
}