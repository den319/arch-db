use std::fs::OpenOptions;
use std::io::{Write, Seek, SeekFrom};

use arch_db::storage::{Storage, SyncPolicy, generate_wal_segment_name};
use arch_db::command::Command;
use arch_db::helper::unique_dir;

#[test]
fn test_wal_reset() {
    let dir = unique_dir("storage/tests");
    let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();

    storage.append(&Command::Set("name".into(), "john".into())).unwrap();
    assert_eq!(storage.load().unwrap().len(), 1);

    storage.reset().unwrap();
    assert_eq!(storage.load().unwrap().len(), 0);

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_wal_recovery_after_restart() {
    let dir = unique_dir("storage/tests");
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        storage.append(&Command::Set("user".into(), "john".into())).unwrap();
    }
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::Set(k, v) => { assert_eq!(k, "user"); assert_eq!(v, "john"); }
            _ => panic!("expected SET"),
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_wal_rotation_after_flush() {
    let dir = unique_dir("storage/tests");
    let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();

    storage.append(&Command::Set("a".into(), "1".into())).unwrap();
    storage.append(&Command::Set("b".into(), "2".into())).unwrap();
    assert_eq!(storage.load().unwrap().len(), 2);

    storage.reset().unwrap();
    assert_eq!(storage.load().unwrap().len(), 0);

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_wal_segment_rotation() {
    let dir = unique_dir("storage/tests");
    let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();

    let large_value = "x".repeat(1024 * 512);
    storage.append(&Command::Set("a".into(), large_value.clone())).unwrap();
    storage.append(&Command::Set("b".into(), large_value)).unwrap();

    assert!(storage.current_segment >= 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_recovery_from_multiple_segments() {
    let dir = unique_dir("storage/tests");
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let large = "x".repeat(1024 * 512);
        storage.append(&Command::Set("a".into(), large.clone())).unwrap();
        storage.append(&Command::Set("b".into(), large)).unwrap();
    }
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        assert!(commands.len() >= 2);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_wal_rotation_preserves_data() {
    let dir = unique_dir("storage/tests");
    let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();

    for i in 0..100 {
        storage.append(&Command::Set(format!("key{}", i), format!("value{}", i))).unwrap();
    }
    assert_eq!(storage.load().unwrap().len(), 100);

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_generate_wal_segment_name() {
    let name = generate_wal_segment_name("mydb", 3);
    assert_eq!(name, "mydb/wal_3.log");
}

#[test]
fn test_wal_checksum_valid_recovery() {
    let dir = unique_dir("checksum_valid");
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        storage.append(&Command::Set("name".into(), "jhon".into())).unwrap();
    }
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::Set(k, v) => { assert_eq!(k, "name"); assert_eq!(v, "jhon"); }
            _ => panic!("expected SET"),
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_wal_checksum_valid_del_recovery() {
    let dir = unique_dir("checksum_del");
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        storage.append(&Command::Del("temp_key".into())).unwrap();
    }
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::Del(k) => { assert_eq!(k, "temp_key"); }
            _ => panic!("expected DEL"),
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_wal_checksum_detects_corruption() {
    let dir = unique_dir("checksum_corrupt");
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        storage.append(&Command::Set("a".into(), "1".into())).unwrap();
    }

    let wal_path = format!("{}/wal_0.log", dir);
    {
        let mut file = OpenOptions::new().write(true).open(&wal_path).unwrap();
        // Seek to last byte and corrupt it, which invalidates the checksum
        file.seek(SeekFrom::End(-1)).unwrap();
        file.write_all(&[255]).unwrap();
    }
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        assert_eq!(commands.len(), 0);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_partial_wal_record_detection() {
    let dir = unique_dir("partial_record");
    let wal_path = format!("{}/wal_0.log", dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Write 4-byte checksum + 4-byte payload_len=100, but no actual payload
    {
        let mut file = OpenOptions::new().create(true).append(true).open(&wal_path).unwrap();
        file.write_all(&[1, 2, 3, 4, 0, 0, 0, 100]).unwrap();
    }
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        assert_eq!(commands.len(), 0);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_detect_corrupted_wal_record() {
    let dir = unique_dir("corrupt_wal");
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        storage.append(&Command::Set("name".into(), "jhon".into())).unwrap();
    }

    // Overwrite the first 4 bytes (the checksum field) with garbage
    let wal_path = generate_wal_segment_name(&dir, 0);
    {
        let mut file = std::fs::OpenOptions::new().write(true).open(&wal_path).unwrap();
        file.write_all(b"XXXX").unwrap();
    }
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        assert_eq!(commands.len(), 0);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

// ────────────────────── NEW CHECKSUM + CORRUPTION TESTS ──────────────────────

#[test]
fn test_checksum_corrupt_payload_bytes() {
    let dir = unique_dir("corrupt_payload");
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        storage.append(&Command::Set("key".into(), "value".into())).unwrap();
    }

    // Open the WAL, skip the 4-byte checksum + 4-byte length, corrupt the payload
    let wal_path = format!("{}/wal_0.log", dir);
    {
        let mut file = OpenOptions::new().write(true).open(&wal_path).unwrap();
        file.seek(SeekFrom::Start(8)).unwrap();             // skip header
        file.write_all(b"XXXX").unwrap();                   // corrupt payload
    }
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        assert_eq!(commands.len(), 0, "payload corruption must cause checksum mismatch");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_checksum_corrupt_payload_length() {
    let dir = unique_dir("corrupt_length");
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        storage.append(&Command::Set("key".into(), "value".into())).unwrap();
    }

    // Corrupt the payload length field (bytes 4-7) so it claims a much larger payload
    let wal_path = format!("{}/wal_0.log", dir);
    {
        let mut file = OpenOptions::new().write(true).open(&wal_path).unwrap();
        file.seek(SeekFrom::Start(4)).unwrap();
        file.write_all(&[0xFF, 0xFF, 0xFF, 0xFF]).unwrap(); // huge payload length
    }
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        assert_eq!(commands.len(), 0, "corrupted payload length must be detected");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_checksum_good_after_bad_stops_at_corruption() {
    // Write a valid record to segment 0, corrupt it, rotate to segment 1,
    // write another valid record. Recovery must skip corrupted segment 0
    // and still recover the valid record in segment 1.
    let dir = unique_dir("good_bad_good");

    let wal_path = format!("{}/wal_0.log", dir);
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        storage.append(&Command::Set("first".into(), "ok".into())).unwrap();
    }

    // Append garbage directly to wal_0.log (after the valid record)
    {
        let mut file = OpenOptions::new().append(true).open(&wal_path).unwrap();
        file.write_all(b"GARBAGE_DATA").unwrap();
    }

    // Open Storage again — it re-opens segment 0 (garbage at end).
    // Explicitly rotate to segment 1, then write another record.
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        storage.rotate_segment().unwrap(); // now on segment 1
        storage.append(&Command::Set("second".into(), "valid".into())).unwrap();
        assert!(storage.current_segment >= 1, "should be on segment 1 or higher");
    }

    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        // "first" may or may not be recovered depending on garbage position
        // "second" must be recovered because it's in segment 1
        let has_second = commands.iter().any(|c| matches!(c, Command::Set(k, _) if k == "second"));
        assert!(has_second, "record in segment 1 after corrupted segment 0 must be recovered");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_checksum_empty_wal_file() {
    let dir = unique_dir("empty_wal");
    let wal_path = format!("{}/wal_0.log", dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Create an empty WAL file
    OpenOptions::new().create(true).write(true).open(&wal_path).unwrap();

    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        assert_eq!(commands.len(), 0, "empty WAL must return 0 commands");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_checksum_truncated_payload() {
    let dir = unique_dir("truncated_payload");
    let wal_path = format!("{}/wal_0.log", dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Write a valid header (checksum + length) but not enough payload bytes
    // payload_len = 50 but we only write 20 bytes after the header
    let mut file = OpenOptions::new().create(true).write(true).open(&wal_path).unwrap();

    // Write fake checksum + length=50
    file.write_all(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 50]).unwrap();
    // Write only 20 bytes of payload
    file.write_all(&[0u8; 20]).unwrap();

    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        assert_eq!(commands.len(), 0, "truncated payload must be detected as partial record");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_checksum_zero_length_payload() {
    let dir = unique_dir("zero_len_payload");
    let wal_path = format!("{}/wal_0.log", dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Write checksum + length=0 (no payload bytes)
    // This is malformed, should be detected
    {
        let mut file = OpenOptions::new().create(true).write(true).open(&wal_path).unwrap();
        // CRC32 of empty bytes = 0x00000000
        file.write_all(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]).unwrap();
    }
    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();
        let commands = storage.load().unwrap();
        assert_eq!(commands.len(), 0, "zero-length payload must not produce a command");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_sync_policy_every_seconds() {
    let dir = unique_dir("sync_every_sec");
    {
        let mut storage = Storage::new(&dir, SyncPolicy::EverySeconds(1)).unwrap();
        storage.append(&Command::Set("x".into(), "y".into())).unwrap();
    }
    {
        let mut storage = Storage::new(&dir, SyncPolicy::EverySeconds(1)).unwrap();
        let commands = storage.load().unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            Command::Set(k, v) => { assert_eq!(k, "x"); assert_eq!(v, "y"); }
            _ => panic!("expected SET"),
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}