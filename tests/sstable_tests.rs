use std::fs;

use arch_db::engine::Value;
use arch_db::sstable::{find_block, read_block, search_sstable, write_sstable};

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
fn test_find_existing_key() {
    let path = "test_find_existing_key.bin";
    let data = sample_data();
    let index = write_sstable(path, &data).unwrap();

    let result = search_sstable(path, &index, "c").unwrap();
    assert!(result.is_some());

    let (_, value) = result.unwrap();
    match value {
        Value::Data(v) => assert_eq!(v, "3"),
        _ => panic!("Expected Data"),
    }
    fs::remove_file(path).unwrap();
}

#[test]
fn test_find_tombstone() {
    let path = "test_find_tombstone.bin";
    let data = sample_data();
    let index = write_sstable(path, &data).unwrap();

    let result = search_sstable(path, &index, "h").unwrap();
    assert!(result.is_some());

    let (_, value) = result.unwrap();
    match value {
        Value::Tombstone => {}
        _ => panic!("Expected Tombstone"),
    }
    fs::remove_file(path).unwrap();
}

#[test]
fn test_missing_key() {
    let path = "test_missing_key.bin";
    let data = sample_data();
    let index = write_sstable(path, &data).unwrap();

    let result = search_sstable(path, &index, "z").unwrap();
    assert!(result.is_none());
    fs::remove_file(path).unwrap();
}

#[test]
fn test_find_correct_block() {
    let path = "test_find_correct_block.bin";
    let data = sample_data();
    let index = write_sstable(path, &data).unwrap();

    let block = find_block(&index, "f");
    assert!(block.is_some());
    let block = block.unwrap();
    assert!(block.start_key <= "f".to_string());
    fs::remove_file(path).unwrap();
}

#[test]
fn test_record_offsets_exist() {
    let path = "test_record_offsets.bin";
    let data = sample_data();
    let index = write_sstable(path, &data).unwrap();

    let mut found = false;
    for block in &index.blocks {
        if block.record_offset.contains_key("c") {
            found = true;
        }
    }
    assert!(found);
    fs::remove_file(path).unwrap();
}

#[test]
fn test_overlap_detection() {
    use std::collections::BTreeMap;
    use bloom::BloomFilter;
    use arch_db::sstable::SSTableIndex;
    use arch_db::sstable_manager::{SSTable, Level};

    let table = SSTable {
        path: String::new(),
        index: SSTableIndex {
            offsets: BTreeMap::new(),
            blocks: vec![],
        },
        bloom: BloomFilter::with_rate(0.01, 8),
        level: Level::L1,
        min_key: "a".to_string(),
        max_key: "f".to_string(),
        file_size: 0,
    };

    assert!(table.overlaps("c", "z"));
    assert!(table.overlaps("a", "f"));
    assert!(table.overlaps("e", "g"));
    assert!(!table.overlaps("g", "z"));
    assert!(!table.overlaps("x", "z"));
}


#[test]
// disk corruption
// bad sectors
// partial writes
// random byte corruption
fn test_detect_corrupted_sstable_block() {

    use std::io::{Seek, SeekFrom, Write};

    let path =
        "test_corrupt_block.bin";

    let data = sample_data();

    write_sstable(
        path,
        &data
    ).unwrap();

    // Corrupt file bytes

    {
        let mut file =
            std::fs::OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap();

        file.seek(
            SeekFrom::Start(10)
        ).unwrap();

        file.write_all(
            b"XXXX"
        ).unwrap();
    }

    let result =
        read_block(path, 0)
            .unwrap();

    assert!(
        result.is_empty()
    );

    std::fs::remove_file(path)
        .unwrap();
}