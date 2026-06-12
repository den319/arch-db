mod command;
mod engine;
mod parser;
mod storage;
mod error;
mod sstable;
mod sstable_manager;
mod helper;
mod cache;

use std::{fs, io::{self, Write}};

use engine::Engine;
use parser::parse;
use command::Command;
use storage::Storage;

use crate::sstable_manager::{Level, init_sstable_counter, next_sstable_id, ManifestRecord};

fn main() {
    init_sstable_counter();

    let mut engine= Engine::new();

    // Replay manifest to restore known SSTables
    {
        let mut sstables = engine.sstables.lock().unwrap();
        let entries = sstables.manifest.load().expect("Failed to load manifest");

        for entry in entries {
            match entry {
                ManifestRecord::AddTable { level, path, .. } => {
                    sstables.load_from_file(&path, level);
                }
                ManifestRecord::RemoveTable { path } => {
                    // Already handled; skip removed tables
                    let _ = path;
                }
            }
        }
    }

    // Discover any SSTables on disk not yet tracked in the manifest
    let dir_entries= fs::read_dir(".").expect("Failed to read directory to load data!");

    for entry in dir_entries {
        let entry= entry.unwrap();

        let name= entry.file_name();
        let name= name.to_string_lossy();

        if !name.ends_with(".bin") {
            continue;
        }

        let level= if name.starts_with("sst_l0_") {
            Level::L0
        } else if name.starts_with("sst_l1_") {
            Level::L1
        } else if name.starts_with("sst_l2_") {
            Level::L2
        } else {
            continue;
        };

        if name.starts_with("sst_") && name.ends_with(".bin") {
            engine.sstables.lock().unwrap().load_from_file(&name, level);
        }
    }
    let mut storage= Storage::new("storage/temp", storage::SyncPolicy::Always).expect("Failed to intialize storage!");

    let commands= storage.load().expect("Failed to load database!");

    for command in commands {

        match command {
            Command::Set(_, _) | Command::Del(_) => {
                engine.execute(command);
            }
            _=> {}
        }
    }

    loop {
        print!("archdb > ");
        io::stdout().flush().unwrap();
        let mut input= String::new();

        io::stdin().read_line(&mut input).expect("Failed to read line");

        let command= parse(&input);

        // println!("input: {:?}, command: {:?}", &input, command);

        match &command {
            Command::Set(_, _) | Command::Del(_) => {
                storage.append(&command).expect("Failed to write log!");
            }
            Command::Exit => {
                if !engine.memtable.is_empty() {
                    let file= format!("sst_l0_{}.bin", next_sstable_id());
    
                    match engine.flush_to_sstable(&file) {
                        Ok(_) => {
                            storage.reset()
                                .expect("Failed to reset WAL");
                        }
                        Err(e) => {
                            println!("Flush failed: {}", e);
                        }
                    }
                    storage.checkpoint().expect("WAL checkpoint failed");
                }
                println!("Bye!");
                break;
            }
            _=>{}
        }

        let is_write = matches!(command, Command::Set(_, _) | Command::Del(_));

        if let Some(output) = engine.execute(command) {
            println!("{}", output);
        }

        if is_write {
            engine.maybe_flush().expect("Flush failed!");
        }
    }
}