use std::{fs::{File, OpenOptions, create_dir_all, remove_file}, io::{Read, Seek, SeekFrom, Write}};

use crate::{command::Command, error::Result};


const WAL_SEGMENT_SIZE: u64 = 1024 * 1024; // 1 MB

#[derive(Clone, Copy, Debug)]
pub enum SyncPolicy {
    Always,
    Never,
    EverySeconds(u64),
}


pub struct Storage {
    file: File,
    pub current_segment: u64,
    current_size: u64,
    base_path: String,
    sync_policy: SyncPolicy,
    last_sync: std::time::Instant,
    last_checkpointed_segment: u64,
}

impl Storage {
    pub fn new(base_apth:&str, sync_policy: SyncPolicy) -> Result<Self> {
        
        create_dir_all(base_apth)?;
        
        // Find the highest existing segment to pick up where we left off
        let mut segment= 0;
        loop {
            let path= generate_wal_segment_name(base_apth, segment + 1);
            if std::path::Path::new(&path).exists() {
                segment += 1;
            } else {
                break;
            }
        }
        
        let path= generate_wal_segment_name(base_apth, segment);


        let file= OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)?;

        let size= file.metadata()?.len();


        Ok(Self { 
            file,
            current_segment: segment,
            current_size: size,
            base_path: base_apth.to_string(),
            sync_policy,
            last_sync: std::time::Instant::now(),
            last_checkpointed_segment: 0,
        })
    }

    pub fn checkpoint(&mut self) -> Result<()> {
        if self.current_segment == 0 {
            return Ok(());
        }

        let safe_until= self.current_segment -1;

        for segment in self.last_checkpointed_segment..=safe_until {
            let path= format!("{}/wal_{}.log", self.base_path, segment);

            if std::path::Path::new(&path).exists() {
                println!("Removing obsolate WAL segment: {}", path);

                remove_file(path)?;
            }
        }

        self.last_checkpointed_segment= self.current_segment;

        Ok(())
    }

    pub fn append(&mut self, command:&Command) -> Result<()> {
        let bytes= command.serialize();

        if self.current_size + bytes.len() as u64 + 8 > WAL_SEGMENT_SIZE {
            self.rotate_segment()?;
        }

        let checksum= crc32fast::hash(&bytes);

        self.file.write_all(&checksum.to_be_bytes())?;
        self.file.write_all(&(bytes.len() as u32).to_be_bytes())?;


        self.file.write_all(&bytes)?;
        self.file.flush()?;

        match self.sync_policy {
            SyncPolicy::Always => {
                self.file.sync_all()?;
            }
            SyncPolicy::Never => {}
            SyncPolicy::EverySeconds(seconds) => {
                if self.last_sync.elapsed() >= std::time::Duration::from_secs(seconds) {
                    self.file.sync_all()?;

                    self.last_sync= std::time::Instant::now();
                }
            }
        }

        self.current_size += bytes.len() as u64 + 8;

        Ok(())
    }

    pub fn load(&mut self) -> Result<Vec<Command>> {

        let mut commands= Vec::new();

        
        for segment in 0..=self.current_segment {
            let path= generate_wal_segment_name(&self.base_path, segment);

            // Skip segments that have been removed by checkpoint
            let mut file = match OpenOptions::new().read(true).open(&path) {
                Ok(f) => f,
                Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            
            let mut bytes= Vec::new();
            
            file.read_to_end(&mut bytes)?;
            
            let mut position=0;

            while position + 8 <= bytes.len() {
                let checksum= u32::from_be_bytes([
                    bytes[position],
                    bytes[position + 1],
                    bytes[position + 2],
                    bytes[position + 3],
                ]);

                position += 4;

                let payload_len= u32::from_be_bytes([
                    bytes[position],
                    bytes[position + 1],
                    bytes[position + 2],
                    bytes[position + 3],
                ]) as usize;

                position += 4;

                if position + payload_len > bytes.len() {
                    println!("Detected partial WAL record");
                    break;
                }

                let payload= &bytes[position..position + payload_len];

                let computed= crc32fast::hash(payload);

                if checksum != computed {
                    println!("WAL checksum mismatch ditected!");
                    break;
                }

                if let Some((command, _))= Command::deserialize(payload) {
                    commands.push(command);
                } 

                position += payload_len;
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


    pub fn rotate_segment(&mut self) -> Result<()> {
        self.current_segment += 1;

        let path= generate_wal_segment_name(&self.base_path, self.current_segment);

        self.file= OpenOptions::new()
                    .create(true)
                    .append(true)
                    .read(true)
                    .open(&path)?;

        self.current_size= 0;

        Ok(())
    }

}



pub fn generate_wal_segment_name(base_path: &str, segment: u64) -> String {
    format!("{}/wal_{}.log", base_path, segment)
}
