use std::collections::BTreeMap;
use std::fs;

use arch_db::bloom_filter::BloomFilter;
use arch_db::command::Command;
use arch_db::engine::{Engine, Value};
use arch_db::sstable::{SSTableIndex, load_index_from_footer, read_sstable, search_sstable, write_sstable};
use arch_db::sstable_manager::{Level, Manifest, ManifestRecord, SSTable, SSTableManager};

fn unique_file(prefix: &str, ext: &str) -> String {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}_{}.{}", prefix, id, ext)
}

#[test]
fn test_add_table_to_correct_level() {
    let mut manager = SSTableManager::new();
    let bloom = BloomFilter::with_rate(0.01, 8);

    let table = SSTable {
        path: "test_add_table_to_correct_level.bin".to_string(),
        index: SSTableIndex {
            offsets: BTreeMap::new(),
            blocks: Vec::new(),
        },
        bloom,
        level: Level::L1,
        min_key: "a".to_string(),
        max_key: "z".to_string(),
        file_size: 0,
    };

    manager.add_table(table);
    assert_eq!(manager.l0.len(), 0);
    assert_eq!(manager.l1.len(), 1);
    assert_eq!(manager.l2.len(), 0);
}

#[test]
fn test_compact_l0_to_l1() {
    let mut manager = SSTableManager::new();

    let data1 = vec![
        ("a".to_string(), Value::Data("1".to_string())),
        ("b".to_string(), Value::Data("2".to_string())),
    ];
    let data2 = vec![
        ("c".to_string(), Value::Data("3".to_string())),
        ("d".to_string(), Value::Data("4".to_string())),
    ];
    let data3 = vec![
        ("e".to_string(), Value::Data("5".to_string())),
        ("f".to_string(), Value::Data("6".to_string())),
    ];

    let file1 = unique_file("compact_l0_test1", "bin");
    let file2 = unique_file("compact_l0_test2", "bin");
    let file3 = unique_file("compact_l0_test3", "bin");

    let index1 = write_sstable(&file1, &data1).unwrap();
    let index2 = write_sstable(&file2, &data2).unwrap();
    let index3 = write_sstable(&file3, &data3).unwrap();

    let mut b1 = BloomFilter::with_rate(0.01, 8);
    b1.insert(&"a".to_string());
    b1.insert(&"b".to_string());
    let mut b2 = BloomFilter::with_rate(0.01, 8);
    b2.insert(&"c".to_string());
    b2.insert(&"d".to_string());
    let mut b3 = BloomFilter::with_rate(0.01, 8);
    b3.insert(&"e".to_string());
    b3.insert(&"f".to_string());

    let s1 = fs::metadata(&file1).unwrap().len();
    let s2 = fs::metadata(&file2).unwrap().len();
    let s3 = fs::metadata(&file3).unwrap().len();

    manager.l0.push(SSTable {
        path: file1,
        index: index1,
        bloom: b1,
        level: Level::L0,
        min_key: "a".into(),
        max_key: "b".into(),
        file_size: s1,
    });
    manager.l0.push(SSTable {
        path: file2,
        index: index2,
        bloom: b2,
        level: Level::L0,
        min_key: "c".into(),
        max_key: "d".into(),
        file_size: s2,
    });
    manager.l0.push(SSTable {
        path: file3,
        index: index3,
        bloom: b3,
        level: Level::L0,
        min_key: "e".into(),
        max_key: "f".into(),
        file_size: s3,
    });

    manager.size_tiered_compact_l0().unwrap();

    assert_eq!(manager.l0.len(), 0);
    assert_eq!(manager.l1.len(), 1);
    std::fs::remove_file(&manager.l1[0].path).unwrap();
}

#[test]
fn test_l0_compaction_keeps_latest_value() {
    let mut manager = SSTableManager::new();

    let old_data = vec![("user".to_string(), Value::Data("old".to_string()))];
    let new_data = vec![("user".to_string(), Value::Data("new".to_string()))];
    let filler_data = vec![("other".to_string(), Value::Data("val".to_string()))];

    let f1 = unique_file("compact_keep_old", "bin");
    let f2 = unique_file("compact_keep_new", "bin");
    let f3 = unique_file("compact_keep_filler", "bin");

    let i1 = write_sstable(&f1, &old_data).unwrap();
    let i2 = write_sstable(&f2, &new_data).unwrap();
    let i3 = write_sstable(&f3, &filler_data).unwrap();

    let mut b1 = BloomFilter::with_rate(0.01, 8);
    b1.insert(&"user".to_string());
    let mut b2 = BloomFilter::with_rate(0.01, 8);
    b2.insert(&"user".to_string());
    let mut b3 = BloomFilter::with_rate(0.01, 8);
    b3.insert(&"other".to_string());

    let s1 = fs::metadata(&f1).unwrap().len();
    let s2 = fs::metadata(&f2).unwrap().len();
    let s3 = fs::metadata(&f3).unwrap().len();

    manager.l0.push(SSTable {
        path: f1,
        index: i1,
        bloom: b1,
        level: Level::L0,
        min_key: "user".into(),
        max_key: "user".into(),
        file_size: s1,
    });
    manager.l0.push(SSTable {
        path: f2,
        index: i2,
        bloom: b2,
        level: Level::L0,
        min_key: "user".into(),
        max_key: "user".into(),
        file_size: s2,
    });
    manager.l0.push(SSTable {
        path: f3,
        index: i3,
        bloom: b3,
        level: Level::L0,
        min_key: "other".into(),
        max_key: "other".into(),
        file_size: s3,
    });

    manager.size_tiered_compact_l0().unwrap();

    let table = &manager.l1[0];
    let result = search_sstable(&table.path, &table.index, "user").unwrap();
    match result {
        Some((_, Value::Data(v))) => assert_eq!(v, "new"),
        _ => panic!("wrong value"),
    }
    let _ = std::fs::remove_file(&table.path);
}

#[test]
fn test_tombstone_survives_compaction() {
    let mut manager = SSTableManager::new();

    let d1 = vec![
        ("a".into(), Value::Data("x".into())),
        ("user".to_string(), Value::Data("john".into())),
    ];
    let d2 = vec![
        ("a".into(), Value::Data("x".into())),
        ("user".to_string(), Value::Tombstone),
    ];
    let d3 = vec![
        ("a".into(), Value::Data("x".into())),
        ("user".to_string(), Value::Data("other".into())),
    ];

    let f1 = unique_file("tombstone_d1", "bin");
    let f2 = unique_file("tombstone_d2", "bin");
    let f3 = unique_file("tombstone_d3", "bin");

    
    let i1 = write_sstable(&f1, &d1).unwrap();
    let i2 = write_sstable(&f2, &d2).unwrap();
    let i3 = write_sstable(&f3, &d3).unwrap();
    
    // println!("reading sstable-f1: {:#?}", read_sstable(&f1).unwrap());
    // println!("reading sstable-f2: {:#?}", read_sstable(&f2).unwrap());
    // println!("reading sstable-f3: {:#?}", read_sstable(&f3).unwrap());
    
    let mut b1 = BloomFilter::with_rate(0.01, 8);
    b1.insert(&"user".to_string());
    b1.insert(&"a".to_string());
    let mut b2 = BloomFilter::with_rate(0.01, 8);
    b2.insert(&"user".to_string());
    b2.insert(&"a".to_string());
    let mut b3 = BloomFilter::with_rate(0.01, 8);
    b3.insert(&"user".to_string());
    b3.insert(&"a".to_string());

    let s1 = fs::metadata(&f1).unwrap().len();
    let s2 = fs::metadata(&f2).unwrap().len();
    let s3 = fs::metadata(&f3).unwrap().len();

    manager.l0.push(SSTable {
        path: f1,
        index: i1,
        bloom: b1,
        level: Level::L0,
        min_key: "a".into(),
        max_key: "user".into(),
        file_size: s1,
    });
    manager.l0.push(SSTable {
        path: f3,
        index: i3,
        bloom: b3,
        level: Level::L0,
        min_key: "a".into(),
        max_key: "user".into(),
        file_size: s3,
    });
    manager.l0.push(SSTable {
        path: f2,
        index: i2,
        bloom: b2,
        level: Level::L0,
        min_key: "a".into(),
        max_key: "user".into(),
        file_size: s2,
    });

    manager.size_tiered_compact_l0().unwrap();

    let data = read_sstable(&manager.l1[0].path).unwrap();
        println!("{:#?}", data);

    let table = &manager.l1[0];
    let result = search_sstable(&table.path, &table.index, "user").unwrap();
    match result {
        Some((_, Value::Tombstone)) => {}
        _ => panic!("expected tombstone"),
    }

    let paths: Vec<String> = manager
        .l0
        .iter()
        .chain(&manager.l1)
        .chain(&manager.l2)
        .map(|t| t.path.clone())
        .collect();
    for p in paths {
        let _ = std::fs::remove_file(&p);
    }
}

#[test]
fn test_load_from_file_into_l1() {
    let mut manager = SSTableManager::new();
    let data = vec![("a".to_string(), Value::Data("1".to_string()))];
    let file = unique_file("load_into_l1", "bin");
    write_sstable(&file, &data).unwrap();
    let file_size = fs::metadata(&file).unwrap().len();
    manager.load_table_metadata(&file, Level::L1, "a".into(), "a".into(), file_size);
    assert_eq!(manager.l0.len(), 0);
    assert_eq!(manager.l1.len(), 1);
    assert_eq!(manager.l2.len(), 0);
    std::fs::remove_file(&file).unwrap();
}

#[test]
fn test_sstable_range_metadata() {
    let data = vec![
        ("apple".into(), Value::Data("1".into())),
        ("banana".into(), Value::Data("2".into())),
        ("orange".into(), Value::Data("3".into())),
    ];
    let file = unique_file("range_meta", "bin");
    let index = write_sstable(&file, &data).unwrap();
    let mut bloom = BloomFilter::with_rate(0.01, 8);
    for (k, _) in &data {
        bloom.insert(k);
    }
    let file_size = fs::metadata(&file).unwrap().len();
    let table = SSTable {
        path: file.clone(),
        index,
        bloom,
        level: Level::L0,
        min_key: "apple".into(),
        max_key: "orange".into(),
        file_size,
    };
    assert_eq!(table.min_key, "apple");
    assert_eq!(table.max_key, "orange");
    std::fs::remove_file(file).unwrap();
}

#[test]
fn test_sstable_range_contains_key() {
    let file = unique_file("range_contains", "bin");
    let table = SSTable {
        path: file,
        index: SSTableIndex {
            offsets: BTreeMap::new(),
            blocks: Vec::new(),
        },
        bloom: BloomFilter::with_rate(0.01, 8),
        level: Level::L1,
        min_key: "apple".into(),
        max_key: "orange".into(),
        file_size: 0,
    };
    assert!(table.contains_key_range("apple"));
    assert!(table.contains_key_range("banana"));
    assert!(table.contains_key_range("orange"));
    assert!(!table.contains_key_range("aardvark"));
    assert!(!table.contains_key_range("zebra"));
}

#[test]
fn test_overlap_detection() {
    let file = unique_file("overlap_detect", "bin");
    let table = SSTable {
        path: file,
        index: SSTableIndex {
            offsets: BTreeMap::new(),
            blocks: Vec::new(),
        },
        bloom: BloomFilter::with_rate(0.01, 8),
        level: Level::L1,
        min_key: "g".into(),
        max_key: "m".into(),
        file_size: 0,
    };
    assert!(table.contains_key_range("g"));
    assert!(table.contains_key_range("k"));
    assert!(table.contains_key_range("m"));
    assert!(!table.contains_key_range("a"));
    assert!(!table.contains_key_range("z"));
}

#[test]
fn test_compact_l1_to_l2_basic() {
    let mut manager = SSTableManager::new();
    let data = vec![
        ("a".into(), Value::Data("1".into())),
        ("b".into(), Value::Data("2".into())),
    ];
    let file = unique_file("l1_to_l2", "bin");
    let index = write_sstable(&file, &data).unwrap();
    let mut bloom = BloomFilter::with_rate(0.01, 8);
    bloom.insert(&"a".to_string());
    bloom.insert(&"b".to_string());
    let file_size = fs::metadata(&file).unwrap().len();
    manager.l1.push(SSTable {
        path: file.clone(),
        index,
        bloom,
        level: Level::L1,
        min_key: "a".into(),
        max_key: "b".into(),
        file_size,
    });
    manager.compact_l1_to_l2().unwrap();
    assert_eq!(manager.l1.len(), 0);
    assert_eq!(manager.l2.len(), 1);
    let result = search_sstable(&manager.l2[0].path, &manager.l2[0].index, "a").unwrap();
    match result {
        Some((_, Value::Data(v))) => assert_eq!(v, "1"),
        _ => panic!("wrong"),
    }
    fs::remove_file(&manager.l2[0].path).unwrap();
}

#[test]
fn test_l1_overwrites_l2_during_compaction() {
    let mut manager = SSTableManager::new();
    let l2_d = vec![("user".into(), Value::Data("old".into()))];
    let l1_d = vec![("user".into(), Value::Data("new".into()))];
    let l2_f = unique_file("overwrite_l2_old", "bin");
    let l1_f = unique_file("overwrite_l2_new", "bin");
    let l2_i = write_sstable(&l2_f, &l2_d).unwrap();
    let l1_i = write_sstable(&l1_f, &l1_d).unwrap();
    let mut b2 = BloomFilter::with_rate(0.01, 8);
    b2.insert(&"user".to_string());
    let mut b1 = BloomFilter::with_rate(0.01, 8);
    b1.insert(&"user".to_string());
    let s2 = fs::metadata(&l2_f).unwrap().len();
    let s1 = fs::metadata(&l1_f).unwrap().len();
    manager.l2.push(SSTable {
        path: l2_f,
        index: l2_i,
        bloom: b2,
        level: Level::L2,
        min_key: "user".into(),
        max_key: "user".into(),
        file_size: s2,
    });
    manager.l1.push(SSTable {
        path: l1_f,
        index: l1_i,
        bloom: b1,
        level: Level::L1,
        min_key: "user".into(),
        max_key: "user".into(),
        file_size: s1,
    });
    manager.compact_l1_to_l2().unwrap();
    let result = search_sstable(&manager.l2[0].path, &manager.l2[0].index, "user").unwrap();
    match result {
        Some((_, Value::Data(v))) => assert_eq!(v, "new"),
        _ => panic!("wrong"),
    }
    fs::remove_file(&manager.l2[0].path).unwrap();
}

#[test]
fn test_tombstone_overwrites_l2_data() {
    let mut manager = SSTableManager::new();
    let l2_d = vec![("user".into(), Value::Data("john".into()))];
    let l1_d = vec![("user".into(), Value::Tombstone)];
    let l2_f = unique_file("tomb_l2", "bin");
    let l1_f = unique_file("tomb_l1", "bin");
    let l2_i = write_sstable(&l2_f, &l2_d).unwrap();
    let l1_i = write_sstable(&l1_f, &l1_d).unwrap();
    let mut b2 = BloomFilter::with_rate(0.01, 8);
    b2.insert(&"user".to_string());
    let mut b1 = BloomFilter::with_rate(0.01, 8);
    b1.insert(&"user".to_string());
    let s2 = fs::metadata(&l2_f).unwrap().len();
    let s1 = fs::metadata(&l1_f).unwrap().len();
    manager.l2.push(SSTable {
        path: l2_f,
        index: l2_i,
        bloom: b2,
        level: Level::L2,
        min_key: "user".into(),
        max_key: "user".into(),
        file_size: s2,
    });
    manager.l1.push(SSTable {
        path: l1_f,
        index: l1_i,
        bloom: b1,
        level: Level::L1,
        min_key: "user".into(),
        max_key: "user".into(),
        file_size: s1,
    });
    manager.compact_l1_to_l2().unwrap();

    // With drop_tombstones=true, the tombstone is applied and removed.
    // L2 should be empty (the old data was removed and tombstone was dropped).
    assert_eq!(manager.l2.len(), 0,
        "Expected tombstone to be dropped and L2 to be empty"
    );
    assert_eq!(manager.l1.len(), 0);
}

#[test]
fn test_non_overlapping_l2_table_survives() {
    let mut manager = SSTableManager::new();
    let l2_d = vec![("x".into(), Value::Data("100".into()))];
    let l1_d = vec![("a".into(), Value::Data("1".into()))];
    let l2_f = unique_file("non_overlap_l2", "bin");
    let l1_f = unique_file("non_overlap_l1", "bin");
    let l2_i = write_sstable(&l2_f, &l2_d).unwrap();
    let l1_i = write_sstable(&l1_f, &l1_d).unwrap();
    let mut b2 = BloomFilter::with_rate(0.01, 8);
    b2.insert(&"x".to_string());
    let mut b1 = BloomFilter::with_rate(0.01, 8);
    b1.insert(&"a".to_string());
    let s2 = fs::metadata(&l2_f).unwrap().len();
    let s1 = fs::metadata(&l1_f).unwrap().len();
    manager.l2.push(SSTable {
        path: l2_f.clone(),
        index: l2_i,
        bloom: b2,
        level: Level::L2,
        min_key: "x".into(),
        max_key: "x".into(),
        file_size: s2,
    });
    manager.l1.push(SSTable {
        path: l1_f,
        index: l1_i,
        bloom: b1,
        level: Level::L1,
        min_key: "a".into(),
        max_key: "a".into(),
        file_size: s1,
    });
    manager.compact_l1_to_l2().unwrap();
    assert_eq!(manager.l2.len(), 2);
    assert!(manager.l2.iter().any(|t| t.min_key == "x"));
    for table in &manager.l2 {
        let _ = fs::remove_file(&table.path);
    }
}

#[test]
fn test_maybe_compact_triggers_l0_to_l1() {
    let mut manager = SSTableManager::new();
    for i in 0..5 {
        let data = vec![(format!("k{}", i), Value::Data(format!("v{}", i)))];
        let file = unique_file("auto_compact", "bin");
        let index = write_sstable(&file, &data).unwrap();
        let mut bloom = BloomFilter::with_rate(0.01, 8);
        bloom.insert(&format!("k{}", i));
        manager.l0.push(SSTable {
            path: file.clone(),
            index,
            bloom,
            level: Level::L0,
            min_key: format!("k{}", i),
            max_key: format!("k{}", i),
            file_size: fs::metadata(&file).unwrap().len(),
        });
    }
    manager.maybe_compact().unwrap();
    assert_eq!(manager.l0.len(), 2);
    assert_eq!(manager.l1.len(), 1);
    for table in manager.l0.iter().chain(manager.l1.iter()) {
        let _ = fs::remove_file(&table.path);
    }
}

#[test]
fn test_sstable_file_size() {
    let data = vec![
        ("a".into(), Value::Data("1".into())),
        ("b".into(), Value::Data("2".into())),
    ];
    let file = unique_file("file_size", "bin");
    write_sstable(&file, &data).unwrap();
    let file_size = fs::metadata(&file).unwrap().len();
    let mut manager = SSTableManager::new();
    manager.load_table_metadata(&file, Level::L1, "a".into(), "b".into(), file_size);
    assert!(manager.l1[0].file_size > 0);
    fs::remove_file(file).unwrap();
}

#[test]
fn test_size_tiered_candidate_selection() {
    let mut manager = SSTableManager::new();
    for i in 0..3 {
        manager.l0.push(SSTable {
            path: format!("t{}.bin", i),
            index: SSTableIndex {
                offsets: BTreeMap::new(),
                blocks: vec![],
            },
            bloom: BloomFilter::with_rate(0.01, 8),
            level: Level::L0,
            min_key: "a".into(),
            max_key: "z".into(),
            file_size: 100,
        });
    }
    let candidates = manager.find_size_tiered_candidates();
    assert_eq!(candidates.len(), 3);
}

#[test]
// Manifest Serialization
fn test_manifest_record_serialization() {
    let record = ManifestRecord::AddTable {
        level: Level::L1,

        path: "sst_1.bin".into(),

        min_key: "a".into(),

        max_key: "z".into(),

        file_size: 123,
    };

    let serialized = record.serialize();

    let deserialized = ManifestRecord::deserialize(&serialized).unwrap();

    match deserialized {
        ManifestRecord::AddTable { level, path, .. } => {
            assert!(matches!(level, Level::L1));

            assert_eq!(path, "sst_1.bin");
        }

        _ => panic!("wrong type"),
    }
}

#[test]
// Manifest Replay
fn test_manifest_append_and_load() {
    let path = "test_manifest.log";

    let mut manifest = Manifest::new(path);

    manifest
        .append(&ManifestRecord::AddTable {
            level: Level::L0,

            path: "sst.bin".into(),

            min_key: "a".into(),

            max_key: "m".into(),

            file_size: 100,
        })
        .unwrap();

    let records = manifest.load_log().unwrap();

    assert_eq!(records.len(), 1);

    std::fs::remove_file(path).unwrap();
}

#[test]
// RemoveTable Replay
fn test_manifest_remove_table() {
    let path = "test_manifest_remove.log";

    let mut manifest = Manifest::new(path);

    manifest
        .append(&ManifestRecord::RemoveTable {
            path: "sst_old.bin".into(),
        })
        .unwrap();

    let records = manifest.load_log().unwrap();

    assert_eq!(records.len(), 1);

    match &records[0] {
        ManifestRecord::RemoveTable { path } => {
            assert_eq!(path, "sst_old.bin");
        }

        _ => panic!("expected REMOVE"),
    }

    std::fs::remove_file(path).unwrap();
}

#[test]
fn test_load_table_metadata() {
    let data = vec![
        ("apple".to_string(), Value::Data("1".to_string())),
        ("banana".to_string(), Value::Data("2".to_string())),
    ];

    let file = unique_file("load_metadata", "bin");

    write_sstable(&file, &data).unwrap();

    let size = fs::metadata(&file).unwrap().len();

    let mut manager = SSTableManager::new();

    manager.load_table_metadata(&file, Level::L0, "apple".into(), "banana".into(), size);

    assert_eq!(manager.l0[0].min_key, "apple");

    assert_eq!(manager.l0[0].max_key, "banana");

    assert!(manager.l0[0].bloom.contains(&"apple".to_string()));

    fs::remove_file(file).unwrap();
}

#[test]
fn test_footer_loaded_index_can_search() {
    let data = vec![("user".to_string(), Value::Data("john".to_string()))];

    let file = unique_file("footer_search", "bin");

    write_sstable(&file, &data).unwrap();

    let index = load_index_from_footer(&file).unwrap();

    let result = search_sstable(&file, &index, "user").unwrap();

    match result {
        Some((_, Value::Data(v))) => assert_eq!(v, "john"),

        _ => panic!("wrong value"),
    }

    fs::remove_file(file).unwrap();
}


#[test]
fn test_manifest_after_first_flush() {
    use std::fs;

    let log_path = unique_file("manifest_first_flush", "log");
    let manager_path = log_path.clone();

    let mut manager = SSTableManager::with_manifest_path(&manager_path);

    let mut b1 = BloomFilter::with_rate(0.01, 8);
    b1.insert(&"a".to_string());

    manager.add_table(SSTable {
        path: "dummy.bin".to_string(),
        index: SSTableIndex { offsets: BTreeMap::new(), blocks: vec![] },
        bloom: b1,
        level: Level::L0,
        min_key: "a".into(),
        max_key: "a".into(),
        file_size: 100,
    });

    let log = fs::read_to_string(&log_path).unwrap();
    println!("{}", log);

    assert!(log.contains("ADD|L0|"));

    let _ = fs::remove_file(&log_path);
}

#[test]
fn test_manifest_multiple_flushes() {
    use std::fs;

    let log_path = unique_file("manifest_multi", "log");
    let manager_path = log_path.clone();

    let mut manager = SSTableManager::with_manifest_path(&manager_path);

    let mut b1 = BloomFilter::with_rate(0.01, 8); b1.insert(&"a".to_string());
    let mut b2 = BloomFilter::with_rate(0.01, 8); b2.insert(&"b".to_string());

    manager.add_table(SSTable {
        path: "t1.bin".to_string(),
        index: SSTableIndex { offsets: BTreeMap::new(), blocks: vec![] },
        bloom: b1,
        level: Level::L0,
        min_key: "a".into(), max_key: "a".into(), file_size: 100,
    });
    manager.add_table(SSTable {
        path: "t2.bin".to_string(),
        index: SSTableIndex { offsets: BTreeMap::new(), blocks: vec![] },
        bloom: b2,
        level: Level::L0,
        min_key: "b".into(), max_key: "b".into(), file_size: 100,
    });

    let log = fs::read_to_string(&log_path).unwrap();
    let add_count = log.lines().filter(|l| l.starts_with("ADD")).count();

    assert_eq!(add_count, 2);

    let _ = fs::remove_file(&log_path);
}


#[test]
fn test_manifest_after_compaction() {
    use std::fs;

    let log_path = unique_file("manifest_compact", "log");
    let ckpt_path = unique_file("manifest_compact_ckpt", "checkpoint");
    let manager_path = log_path.clone();

    let mut manager = SSTableManager::with_manifest_path(&manager_path);
    // Override checkpoint path to be unique too
    manager.manifest.set_checkpoint_path(&ckpt_path);

    for i in 0..4 {
        let data = vec![(format!("k{}", i), Value::Data(format!("v{}", i)))];
        let file = unique_file("compact_data", "bin");
        let index = write_sstable(&file, &data).unwrap();
        let mut bloom = BloomFilter::with_rate(0.01, 8);
        bloom.insert(&format!("k{}", i));
        let size = fs::metadata(&file).unwrap().len();

        manager.l0.push(SSTable {
            path: file.clone(),
            index,
            bloom,
            level: Level::L0,
            min_key: format!("k{}", i),
            max_key: format!("k{}", i),
            file_size: size,
        });
    }

    // Ensure manifest has ADD records for the candidate tables
    // (size_tiered_compact_l0 writes REMOVE + install_table writes ADDs)
    // Instead, let's add them via add_table first
    // Actually the test just needs to verify that compaction writes REMOVE
    // So let's push tables directly and call compact
    manager.size_tiered_compact_l0().unwrap();

    // Wait a bit for background ops
    std::thread::sleep(std::time::Duration::from_millis(100));

    let log = fs::read_to_string(&log_path).unwrap();
    println!("{}", log);

    assert!(log.contains("REMOVE"));

    // Cleanup
    for table in manager.l0.iter().chain(&manager.l1).chain(&manager.l2) {
        let _ = fs::remove_file(&table.path);
    }
    let _ = fs::remove_file(&log_path);
    let _ = fs::remove_file(&ckpt_path);
}


#[test]
fn test_manifest_recovery() {
    use std::fs;

    let log_path = unique_file("manifest_recovery", "log");
    let manager_path = log_path.clone();

    // Create an SSTable with data, then add it to a manager with the unique manifest
    let data = vec![("a".into(), Value::Data("1".into()))];
    let sst_file = unique_file("recovery_data", "bin");
    write_sstable(&sst_file, &data).unwrap();
    let size = fs::metadata(&sst_file).unwrap().len();
    let index = load_index_from_footer(&sst_file).unwrap();
    let bloom = BloomFilter::with_rate(0.01, 8);

    let mut table = SSTable {
        path: sst_file.clone(),
        index,
        bloom: bloom.clone(),
        level: Level::L0,
        min_key: "a".into(),
        max_key: "a".into(),
        file_size: size,
    };

    // Use add_table which writes to manifest
    {
        let mut manager = SSTableManager::with_manifest_path(&manager_path);
        manager.add_table(table);
        // Dropping here simulates restart
    }

    // Now reload from manifest
    let mut new_manager = SSTableManager::with_manifest_path(&manager_path);

    let records = new_manager.manifest.load_log().unwrap();

    for record in records {
        if let ManifestRecord::AddTable {
            level,
            path,
            min_key,
            max_key,
            file_size,
        } = record
        {
            new_manager.load_table_metadata(
                &path,
                level,
                min_key,
                max_key,
                file_size,
            );
        }
    }

    assert_eq!(new_manager.l0.len(), 1);

    let _ = fs::remove_file(&sst_file);
    let _ = fs::remove_file(&log_path);
}


#[test]
fn test_manifest_checkpoint() {
    use std::fs;

    let log_path = unique_file("checkpoint_log", "log");
    let ckpt_path = unique_file("checkpoint_file", "checkpoint");

    let mut manager = SSTableManager::with_manifest_path(&log_path);
    manager.manifest.set_checkpoint_path(&ckpt_path);

    let mut b1 = BloomFilter::with_rate(0.01, 8); b1.insert(&"a".to_string());

    manager.add_table(SSTable {
        path: "test.bin".to_string(),
        index: SSTableIndex { offsets: BTreeMap::new(), blocks: vec![] },
        bloom: b1,
        level: Level::L0,
        min_key: "a".into(), max_key: "a".into(), file_size: 100,
    });

    // Force a checkpoint
    manager.checkpoint_manifest().unwrap();

    let checkpoint = fs::read_to_string(&ckpt_path).unwrap();
    println!("{}", checkpoint);

    assert!(!checkpoint.contains("REMOVE"));

    let _ = fs::remove_file(&log_path);
    let _ = fs::remove_file(&ckpt_path);
}


#[test]
fn test_manifest_log_truncated_after_checkpoint() {
    use std::fs;

    let log_path = unique_file("truncated_log", "log");
    let ckpt_path = unique_file("truncated_ckpt", "checkpoint");

    let mut manager = SSTableManager::with_manifest_path(&log_path);
    manager.manifest.set_checkpoint_path(&ckpt_path);

    let mut b1 = BloomFilter::with_rate(0.01, 8); b1.insert(&"a".to_string());

    manager.add_table(SSTable {
        path: "test.bin".to_string(),
        index: SSTableIndex { offsets: BTreeMap::new(), blocks: vec![] },
        bloom: b1,
        level: Level::L0,
        min_key: "a".into(), max_key: "a".into(), file_size: 100,
    });

    // Force a checkpoint — this should truncate the log
    manager.checkpoint_manifest().unwrap();

    let log = fs::read_to_string(&log_path).unwrap();
    println!("log content after checkpoint: '{}'", log);

    assert!(log.is_empty());

    let _ = fs::remove_file(&log_path);
    let _ = fs::remove_file(&ckpt_path);
}


#[test]
fn test_compaction_splits_output_into_multiple_tables() {
    let manager = create_large_compacted_manager();

    assert!(
        manager.l1.len() > 1,
        "Expected multiple SSTables after compaction, got {}",
        manager.l1.len()
    );

    cleanup_manager(&manager);
}


#[test]
fn test_compaction_split_tables_are_sorted() {
    let manager = create_large_compacted_manager();

    for table in &manager.l1 {

        let data = read_sstable(&table.path).unwrap();

        for window in data.windows(2) {
            assert!(window[0].0 < window[1].0);
        }
    }

    cleanup_manager(&manager);
}


#[test]
fn test_split_tables_have_non_overlapping_key_ranges() {
    let manager = create_large_compacted_manager();

    for pair in manager.l1.windows(2) {

        let left = &pair[0];
        let right = &pair[1];

        assert!(left.max_key < right.min_key);
    }

    cleanup_manager(&manager);
}


#[test]
fn test_split_tables_preserve_all_records() {

    let manager = create_large_compacted_manager();

    let mut merged = Vec::new();

    for table in &manager.l1 {
        merged.extend(read_sstable(&table.path).unwrap());
    }

    for window in merged.windows(2) {
        assert!(window[0].0 < window[1].0);
    }

    assert_eq!(merged.len(), 100);

    cleanup_manager(&manager);
}

fn create_large_compacted_manager() -> SSTableManager {
    let mut manager = SSTableManager::new();

    // Push 3 L0 tables so size_tiered_compact_l0 triggers (needs >= 3)
    for batch in 0..3 {
        let mut data = Vec::new();
        for i in 0..100 {
            data.push((
                format!("key_{:03}", i),
                Value::Data("x".repeat(50)),
            ));
        }

        let file = unique_file(&format!("split_input_batch{}", batch), "bin");
        let index = write_sstable(&file, &data).unwrap();
        let mut bloom = BloomFilter::with_rate(0.01, data.len() as u32);
        for (k, _) in &data {
            bloom.insert(k);
        }
        let size = fs::metadata(&file).unwrap().len();

        manager.l0.push(SSTable {
            path: file,
            index,
            bloom,
            level: Level::L0,
            min_key: "key_000".into(),
            max_key: "key_099".into(),
            file_size: size,
        });
    }

    manager.size_tiered_compact_l0().unwrap();

    assert!(
        manager.l1.len() > 1,
        "Expected multiple SSTables after compaction, got {}",
        manager.l1.len()
    );

    manager
}

fn cleanup_manager(manager: &SSTableManager) {
    for table in manager
        .l0
        .iter()
        .chain(manager.l1.iter())
        .chain(manager.l2.iter())
    {
        let _ = fs::remove_file(&table.path);
    }
}

#[test]
fn test_no_empty_tables_created() {
    let manager = create_large_compacted_manager();

    for table in &manager.l1 {
        let data = read_sstable(&table.path).unwrap();
        assert!(!data.is_empty());
    }

    cleanup_manager(&manager);
}
