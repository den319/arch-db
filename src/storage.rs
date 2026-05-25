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

        self.file.write_all(&bytes);
        self.file.flush()?;

        Ok(())
    }

    pub fn load(&mut self) -> Result<Vec<Command>> {
        self.file.seek(SeekFrom::Start(0));

        let mut bytes= Vec::new();
        self.file.read_to_end(&mut bytes);

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
}