use std::fs::OpenOptions;
use std::io::Write;

use arch_db::storage::{Storage, SyncPolicy, generate_wal_segment_name};
use arch_db::command::Command;
use arch_db::helper::unique_dir;

#[test]
fn test_wal_reset() {
    let dir = unique_dir("storage/test");

    let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();

    storage
        .append(&Command::Set("name".into(), "john".into()))
        .unwrap();

    let commands = storage.load().unwrap();

    assert_eq!(commands.len(), 1);

    storage.reset().unwrap();

    let commands = storage.load().unwrap();

    assert_eq!(commands.len(), 0);

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_wal_recovery_after_restart() {
    let dir = unique_dir("storage/test");

    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();

        storage
            .append(&Command::Set("user".into(), "john".into()))
            .unwrap();
    }

    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();

        let commands = storage.load().unwrap();

        assert_eq!(commands.len(), 1);

        match &commands[0] {
            Command::Set(k, v) => {
                assert_eq!(k, "user");
                assert_eq!(v, "john");
            }
            _ => panic!("expected SET"),
        }
    }

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_wal_rotation_after_flush() {
    let dir = unique_dir("storage/test");

    let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();

    storage
        .append(&Command::Set("a".into(), "1".into()))
        .unwrap();

    storage
        .append(&Command::Set("b".into(), "2".into()))
        .unwrap();

    assert_eq!(storage.load().unwrap().len(), 2);

    storage.reset().unwrap();

    assert_eq!(storage.load().unwrap().len(), 0);

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
// WAL rotation triggers
// segment increments
// files created correctly
fn test_wal_segment_rotation() {
    let dir = unique_dir("storage/test");

    let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();

    let large_value = "x".repeat(1024 * 512);

    storage
        .append(&Command::Set("a".into(), large_value.clone()))
        .unwrap();

    storage
        .append(&Command::Set("b".into(), large_value))
        .unwrap();

    assert!(storage.current_segment >= 1);

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
// replay across segments works
// recovery architecture valid
fn test_recovery_from_multiple_segments() {
    let dir = unique_dir("storage/test");

    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();

        let large = "x".repeat(1024 * 512);

        storage
            .append(&Command::Set("a".into(), large.clone()))
            .unwrap();

        storage
            .append(&Command::Set("b".into(), large))
            .unwrap();
    }

    {
        let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();

        let commands = storage.load().unwrap();

        assert!(commands.len() >= 2);
    }

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
// Rotation Preserves Writes
fn test_wal_rotation_preserves_data() {
    let dir = unique_dir("storage/test");

    let mut storage = Storage::new(&dir, SyncPolicy::Never).unwrap();

    for i in 0..100 {
        storage
            .append(&Command::Set(format!("key{}", i), format!("value{}", i)))
            .unwrap();
    }

    let commands = storage.load().unwrap();

    assert_eq!(commands.len(), 100);

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn test_generate_wal_segment_name() {
    let name = generate_wal_segment_name("mydb", 3);
    assert_eq!(name, "mydb/wal_3.log");
}

#[test]
// Valid WAL Replay Still Works
fn test_wal_checksum_valid_recovery() {

    let dir =
        unique_dir(
            "checksum_valid"
        );

    {
        let mut storage =
            Storage::new(&dir, SyncPolicy::Never)
                .unwrap();

        storage.append(
            &Command::Set(
                "name".into(),
                "jhon".into(),
            )
        ).unwrap();
    }

    {
        let mut storage =
            Storage::new(&dir, SyncPolicy::Never)
                .unwrap();

        let commands =
            storage.load().unwrap();

        assert_eq!(commands.len(), 1);

        match &commands[0] {
            Command::Set(k, v) => {
                assert_eq!(k, "name");
                assert_eq!(v, "jhon");
            }
            _ => panic!("expected SET"),
        }
    }

    std::fs::remove_dir_all(dir)
        .unwrap();
}

#[test]
// detect corruption
// reject invalid records
fn test_wal_checksum_detects_corruption() {

    let dir =
        unique_dir(
            "checksum_corrupt"
        );

    {
        let mut storage =
            Storage::new(&dir, SyncPolicy::Never)
                .unwrap();

        storage.append(
            &Command::Set(
                "a".into(),
                "1".into(),
            )
        ).unwrap();
    }

    let wal_path =
        format!("{}/wal_0.log", dir);

    {
        use std::io::{Seek, SeekFrom};

        let mut file =
            OpenOptions::new()
                .write(true)
                .open(&wal_path)
                .unwrap();

        file.seek(
            SeekFrom::End(-1)
        ).unwrap();

        file.write_all(&[255])
            .unwrap();
    }

    {
        let mut storage =
            Storage::new(&dir, SyncPolicy::Never)
                .unwrap();

        let commands =
            storage.load().unwrap();

        assert_eq!(commands.len(), 0);
    }

    std::fs::remove_dir_all(dir)
        .unwrap();
}

#[test]
// Partial WAL Write Detection
fn test_partial_wal_record_detection() {

    let dir =
        unique_dir(
            "partial_record"
        );

    let wal_path =
        format!("{}/wal_0.log", dir);

    std::fs::create_dir_all(&dir)
        .unwrap();

    {
        let mut file =
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&wal_path)
                .unwrap();

        file.write_all(&[
            1,2,3,4,
            0,0,0,100
        ]).unwrap();
    }

    {
        let mut storage =
            Storage::new(&dir, SyncPolicy::Never)
                .unwrap();

        let commands =
            storage.load().unwrap();

        assert_eq!(commands.len(), 0);
    }

    std::fs::remove_dir_all(dir)
        .unwrap();
}

#[test]
// Always Policy Forces Sync
// Basic durability path still works.
fn test_sync_policy_always() {

    let dir =
        unique_dir(
            "sync_always"
        );

    let mut storage =
        Storage::new(
            &dir,
            SyncPolicy::Always,
        ).unwrap();

    storage.append(
        &Command::Set(
            "a".into(),
            "1".into(),
        )
    ).unwrap();

    let commands =
        storage.load().unwrap();

    assert_eq!(commands.len(), 1);

    std::fs::remove_dir_all(dir)
        .unwrap();
}


#[test]
// Never Policy Works
// group commit batching
fn test_sync_policy_never() {

    let dir =
        unique_dir(
            "sync_never"
        );

    let mut storage =
        Storage::new(
            &dir,
            SyncPolicy::Never,
        ).unwrap();

    storage.append(
        &Command::Set(
            "b".into(),
            "2".into(),
        )
    ).unwrap();

    let commands =
        storage.load().unwrap();

    assert_eq!(commands.len(), 1);

    std::fs::remove_dir_all(dir)
        .unwrap();
}



#[test]
// Checkpoint Removes Old WAL Segments
// Old WAL segments are actually deleted.
fn test_checkpoint_removes_old_segments() {

    let dir =
        unique_dir(
            "checkpoint_cleanup"
        );

    let mut storage =
        Storage::new(
            &dir,
            SyncPolicy::Always,
        ).unwrap();

    for i in 0..10 {

        storage.append(
            &Command::Set(
                format!("k{}", i),
                format!("v{}", i),
            )
        ).unwrap();
    }

    storage.rotate_segment()
        .unwrap();

    let wal0 =
        format!("{}/wal_0.log", dir);

    assert!(
        std::path::Path::new(&wal0)
            .exists()
    );

    storage.checkpoint()
        .unwrap();

    assert!(
        !std::path::Path::new(&wal0)
            .exists()
    );

    std::fs::remove_dir_all(dir)
        .unwrap();
}

#[test]
// Active Segment Must Survive
// catastrophic durability loss
fn test_checkpoint_keeps_active_segment() {

    let dir =
        unique_dir(
            "checkpoint_active"
        );

    let mut storage =
        Storage::new(
            &dir,
            SyncPolicy::Always,
        ).unwrap();

    storage.append(
        &Command::Set(
            "a".into(),
            "1".into(),
        )
    ).unwrap();

    storage.checkpoint()
        .unwrap();

    let active =
        format!("{}/wal_0.log", dir);

    assert!(
        std::path::Path::new(&active)
            .exists()
    );

    std::fs::remove_dir_all(dir)
        .unwrap();
}


#[test]
// Recovery Still Works After Checkpoint
fn test_recovery_after_checkpoint() {

    let dir =
        unique_dir(
            "checkpoint_recovery"
        );

    {
        let mut storage =
            Storage::new(
                &dir,
                SyncPolicy::Always,
            ).unwrap();

        storage.append(
            &Command::Set(
                "name".into(),
                "jhon".into(),
            )
        ).unwrap();

        storage.rotate_segment()
            .unwrap();

        storage.checkpoint()
            .unwrap();

        storage.append(
            &Command::Set(
                "city".into(),
                "chicago".into(),
            )
        ).unwrap();
    }

    {
        let mut storage =
            Storage::new(
                &dir,
                SyncPolicy::Always,
            ).unwrap();

        let commands =
            storage.load().unwrap();

        assert_eq!(commands.len(), 1);

        match &commands[0] {
            Command::Set(k, v) => {

                assert_eq!(k, "city");
                assert_eq!(v, "chicago");
            }

            _ => panic!("expected SET"),
        }
    }

    std::fs::remove_dir_all(&dir)
        .unwrap();
}