use std::fs;

use arch_db::engine::Value;
use arch_db::sstable::{write_sstable, read_sstable};

fn sample_data() -> Vec<(String, Value)> {
    vec![
        ("a".to_string(), Value::Data("1".to_string())),
        ("b".to_string(), Value::Data("2".to_string())),
        ("c".to_string(), Value::Data("3".to_string())),
        ("d".to_string(), Value::Data("4".to_string())),
        ("e".to_string(), Value::Data("5".to_string())),
        ("f".to_string(), Value::Data("6".to_string())),
        ("g".to_string(), Value::Data("7".to_string())),
        ("h".to_string(), Value::Tombstone),
    ]
}

#[test]
fn test_write_and_read_sstable() {
    let path = "test_write_and_read.bin";
    let data = sample_data();
    write_sstable(path, &data).unwrap();
    let loaded = read_sstable(path).unwrap();
    assert_eq!(loaded.len(), data.len());
    fs::remove_file(path).unwrap();
}

#[test]
fn test_multiple_blocks_created() {
    let path = "test_multiple_blocks.bin";
    let data = sample_data();
    let index = write_sstable(path, &data).unwrap();
    assert!(index.blocks.len() > 1);
    fs::remove_file(path).unwrap();
}