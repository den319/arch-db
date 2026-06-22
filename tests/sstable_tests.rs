use std::fs;

use arch_db::engine::Value;
use arch_db::helper::unique_file;
use arch_db::sstable::{SSTableIterator, SSTableWriter, find_block, load_bloom_from_footer, load_index_from_footer, read_block, read_footer, search_sstable, write_sstable};
use arch_db::sstable_manager::SSTable;

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
    use arch_db::bloom_filter::BloomFilter;
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

        // Corrupt somewhere in the first block's compressed data (past the 8-byte file header
        // and 12-byte block header = offset 20)
        file.seek(
            SeekFrom::Start(25)
        ).unwrap();

        file.write_all(
            b"XXXX"
        ).unwrap();
    }

    // Read first block — the block is at offset 8 (HEADER_SIZE) past the file header
    let result =
        read_block(path, 8)
            .unwrap();

    assert!(
        result.is_empty()
    );

    std::fs::remove_file(path)
        .unwrap();
}



#[test]
fn test_footer_roundtrip() {
    let path = "test_footer_roundtrip.bin";

    let data = vec![
        ("a".to_string(), Value::Data("1".into())),
        ("b".to_string(), Value::Data("2".into())),
    ];

    write_sstable(path, &data).unwrap();

    let footer =
        read_footer(path).unwrap();

    assert!(footer.index_offset > 0);

    std::fs::remove_file(path).unwrap();
}

#[test]
fn test_load_index_from_footer() {
    let path =
        "test_load_index_from_footer.bin";

    let data = vec![
        ("a".to_string(), Value::Data("1".into())),
        ("b".to_string(), Value::Data("2".into())),
    ];

    let original_index =
        write_sstable(path, &data).unwrap();

    let loaded_index =
        load_index_from_footer(path)
            .unwrap();

    assert_eq!(
        original_index.blocks.len(),
        loaded_index.blocks.len()
    );

    assert_eq!(
        original_index.blocks[0].start_key,
        loaded_index.blocks[0].start_key
    );

    std::fs::remove_file(path).unwrap();
}

#[test]
fn test_footer_metadata_roundtrip() {

    let data = vec![
        (
            "apple".to_string(),
            Value::Data("1".to_string())
        ),
        (
            "banana".to_string(),
            Value::Data("2".to_string())
        ),
    ];

    let file =
        unique_file(
            "footer_roundtrip",
            "bin"
        );

    write_sstable(
        &file,
        &data
    ).unwrap();

    let footer =
        read_footer(&file)
            .unwrap();

    assert!(
        footer.index_size > 0
    );

    assert!(
        footer.bloom_size > 0
    );

    std::fs::remove_file(file)
        .unwrap();
}

#[test]
fn test_load_bloom_from_footer() {

    let data = vec![
        (
            "apple".to_string(),
            Value::Data("1".to_string())
        ),
        (
            "banana".to_string(),
            Value::Data("2".to_string())
        ),
    ];

    let file =
        unique_file(
            "footer_bloom",
            "bin"
        );

    write_sstable(
        &file,
        &data
    ).unwrap();

    let bloom =
        load_bloom_from_footer(
            &file
        ).unwrap();

    assert!(
        bloom.contains(&"apple".to_string())
    );

    assert!(
        bloom.contains(&"banana".to_string())
    );

    std::fs::remove_file(file)
        .unwrap();
}

#[test]
fn test_sstable_iterator() {
    let path = "iterator_test.bin";

    let mut data = Vec::new();

    for i in 0..20 {
        data.push((
            format!("key{:02}", i),
            Value::Data(format!("value{}", i)),
        ));
    }

    let index = write_sstable(path, &data).unwrap();

    let mut iter = SSTableIterator::new(path, index).unwrap();

    let mut result = Vec::new();

    while let Some(record) = iter.next().unwrap() {
        result.push(record);
    }

    assert_eq!(result.len(), data.len());

    for i in 0..20 {
        assert_eq!(result[i].key, format!("key{:02}", i));

        match &result[i].value {
            Value::Data(v) => {
                assert_eq!(v, &format!("value{}", i));
            }
            _ => panic!("Unexpected tombstone"),
        }
    }

    fs::remove_file(path).unwrap();
}

#[test]
fn test_seek_existing_key() {
    let path = "seek_existing.bin";

    let mut data = Vec::new();
    for i in 0..20 {
        data.push((
            format!("key{:02}", i),
            Value::Data(format!("value{}", i)),
        ));
    }

    let index = write_sstable(path, &data).unwrap();

    let mut iter = SSTableIterator::new(path, index).unwrap();

    // Seek to an existing key
    iter.seek("key10").unwrap();
    let record = iter.next().unwrap().expect("expected a record after seek");
    assert_eq!(record.key, "key10");
    match record.value {
        Value::Data(v) => assert_eq!(v, "value10"),
        _ => panic!("Expected Data"),
    }

    fs::remove_file(path).unwrap();
}

#[test]
fn test_seek_non_existent_key() {
    let path = "seek_nonexistent.bin";

    let mut data = Vec::new();
    for i in 0..20 {
        data.push((
            format!("key{:02}", i),
            Value::Data(format!("value{}", i)),
        ));
    }

    let index = write_sstable(path, &data).unwrap();

    let mut iter = SSTableIterator::new(path, index).unwrap();

    // Seek to a key that does not exist, but falls between existing keys
    iter.seek("key10a").unwrap();
    let record = iter.next().unwrap().expect("expected a record after seek");
    assert_eq!(record.key, "key11");
    match record.value {
        Value::Data(v) => assert_eq!(v, "value11"),
        _ => panic!("Expected Data"),
    }

    fs::remove_file(path).unwrap();
}

#[test]
fn test_sstable_writer_single_record() -> Result<()> {
    let path = "test_single_writer.sst";

    let mut writer = SSTableWriter::new(path, 1)?;

    writer.append(
        "apple".to_string(),
        Value::Data("100".to_string()),
    )?;

    writer.finish()?;

    let table = load_from_file(path)?;

    assert_eq!(
        search_sstable(&table, "apple")?,
        Some(Value::Data("100".to_string()))
    );

    std::fs::remove_file(path)?;

    Ok(())
}

#[test]
fn test_sstable_writer_multiple_records() -> Result<()> {
    let path = "test_multiple_writer.sst";

    let mut writer = SSTableWriter::new(path, 10)?;

    for i in 0..10 {
        writer.append(
            format!("key{:02}", i),
            Value::Data(format!("value{}", i)),
        )?;
    }

    writer.finish()?;

    let table = SSTable::load_from_file(path)?;

    for i in 0..10 {
        assert_eq!(
            search_sstable(&table, &format!("key{:02}", i))?,
            Some(Value::Data(format!("value{}", i)))
        );
    }

    std::fs::remove_file(path)?;

    Ok(())
}

#[test]
fn test_sstable_writer_multiple_blocks() -> Result<()> {
    let path = "test_blocks_writer.sst";

    let mut writer = SSTableWriter::new(path, 500);

    for i in 0..500 {
        writer.append(
            format!("key{:04}", i),
            Value::Data("abcdefghijklmnopqrstuvwxyz".repeat(4)),
        )?;
    }

    let index = writer.finish()?;

    assert!(index.blocks.len() > 1);

    let table = SSTable::load_from_file(path)?;

    for i in [0, 50, 100, 200, 300, 499] {
        assert_eq!(
            search_sstable(&table, &format!("key{:04}", i))?,
            Some(Value::Data("abcdefghijklmnopqrstuvwxyz".repeat(4)))
        );
    }

    std::fs::remove_file(path)?;

    Ok(())
}

#[test]
fn test_sstable_writer_empty() -> Result<()> {
    let path = "test_empty_writer.sst";

    let writer = SSTableWriter::new(path, 1)?;

    let index = writer.finish()?;

    assert!(index.offsets.is_empty());
    assert!(index.blocks.is_empty());

    let table = SSTable::load_from_file(path)?;

    assert!(search_sstable(&table, "anything")?.is_none());

    std::fs::remove_file(path)?;

    Ok(())
}

#[test]
fn test_sstable_writer_tombstones() -> Result<()> {
    let path = "test_tombstone_writer.sst";

    let mut writer = SSTableWriter::new(path, 3)?;

    writer.append(
        "apple".into(),
        Value::Data("10".into()),
    )?;

    writer.append(
        "banana".into(),
        Value::Tombstone,
    )?;

    writer.append(
        "cat".into(),
        Value::Data("30".into()),
    )?;

    writer.finish()?;

    let table = SSTable::load_from_file(path)?;

    assert_eq!(
        search_sstable(&table, "banana")?,
        Some(Value::Tombstone)
    );

    std::fs::remove_file(path)?;

    Ok(())
}

#[test]
fn test_writer_matches_old_write_sstable() -> Result<()> {
    let old_path = "old.sst";
    let new_path = "new.sst";

    let data = vec![
        ("apple".to_string(), Value::Data("1".to_string())),
        ("banana".to_string(), Value::Data("2".to_string())),
        ("cat".to_string(), Value::Tombstone),
        ("dog".to_string(), Value::Data("4".to_string())),
    ];

    let old_index = write_sstable(old_path, &data)?;

    let mut writer = SSTableWriter::new(new_path, data.len())?;

    for (k, v) in &data {
        writer.append(k.clone(), v.clone())?;
    }

    let new_index = writer.finish()?;

    assert_eq!(old_index.offsets, new_index.offsets);
    assert_eq!(old_index.blocks.len(), new_index.blocks.len());

    let old_table = SSTable::load_from_file(old_path)?;
    let new_table = SSTable::load_from_file(new_path)?;

    for (key, value) in &data {
        assert_eq!(search_sstable(&old_table, key)?, Some(value.clone()));
        assert_eq!(search_sstable(&new_table, key)?, Some(value.clone()));
    }

    std::fs::remove_file(old_path)?;
    std::fs::remove_file(new_path)?;

    Ok(())
}

#[test]
fn test_finish_without_append() -> Result<()> {
    let path = "empty_finish.sst";

    let writer = SSTableWriter::new(path, 10)?;

    let index = writer.finish()?;

    assert!(index.blocks.is_empty());
    assert!(index.offsets.is_empty());

    std::fs::remove_file(path)?;

    Ok(())
}