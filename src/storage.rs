use std::{fs::{File, OpenOptions}, io::{Read, Seek, SeekFrom, Write}};

use crate::{command::Command, error::Result};

pub struct Storage {
    file: File
}

impl Storage {
    pub fn new(path:&str) -> Result<Self> {
        let file= OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)?;

        Ok(Self { file })
    }

    pub fn append(&mut self, command:&Command) -> Result<()> {
        let bytes= command.serialize();

        self.file.write_all(&bytes)?;
        self.file.flush()?;

        Ok(())
    }

    pub fn load(&mut self) -> Result<Vec<Command>> {
        self.file.seek(SeekFrom::Start(0))?;

        let mut bytes= Vec::new();
        self.file.read_to_end(&mut bytes)?;

        let mut commands= Vec::new();

        let mut position=0;

        while position < bytes.len() {
            if let Some((command, consumed))= Command::deserialize(&bytes[position..]) {
                commands.push(command);
                position += consumed;
            } else {
                break;
            }
        }

        Ok(commands)
    }

    pub fn reset(&mut self) -> Result<()> {
        self.file.set_len(0)?;
        self.file.sync_all()?;
        self.file.seek(SeekFrom::Start(0))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{command::Command, helper::unique_file};

    #[test]
    fn test_wal_reset() {
        let path = "test_wal_reset.log";

        let mut storage =
            Storage::new(path).unwrap();

        storage
            .append(&Command::Set(
                "name".into(),
                "john".into()
            ))
            .unwrap();

        let commands =
            storage.load().unwrap();

        assert_eq!(commands.len(), 1);

        storage.reset().unwrap();

        let commands =
            storage.load().unwrap();

        assert_eq!(commands.len(), 0);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
fn test_wal_recovery_after_restart() {
    let path = "test_recovery.log";

    {
        let mut storage =
            Storage::new(path).unwrap();

        storage
            .append(&Command::Set(
                "user".into(),
                "john".into()
            ))
            .unwrap();
    }

    {
        let mut storage =
            Storage::new(path).unwrap();

        let commands =
            storage.load().unwrap();

        assert_eq!(commands.len(), 1);

        match &commands[0] {
            Command::Set(k,v) => {
                assert_eq!(k, "user");
                assert_eq!(v, "john");
            }
            _ => panic!("expected SET"),
        }
    }

    std::fs::remove_file(path).unwrap();
}

#[test]
fn test_wal_rotation_after_flush() {
    let path = unique_file("test_rotation", "log");

    let mut storage =
        Storage::new(&path).unwrap();

    storage
        .append(&Command::Set(
            "a".into(),
            "1".into()
        ))
        .unwrap();

    storage
        .append(&Command::Set(
            "b".into(),
            "2".into()
        ))
        .unwrap();

    assert_eq!(
        storage.load().unwrap().len(),
        2
    );

    storage.reset().unwrap();

    assert_eq!(
        storage.load().unwrap().len(),
        0
    );

    std::fs::remove_file(path).unwrap();
}


}