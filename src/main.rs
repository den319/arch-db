mod command;
mod engine;
mod parser;
mod storage;
mod error;
mod sstable;
mod sstable_manager;

use std::{fs, io::{self, Write}};

use engine::Engine;
use parser::parse;
use command::Command;
use storage::Storage;
use sstable_manager::discover_sstables;

fn main() {
    let mut engine= Engine::new();

    let entries= fs::read_dir(".").expect("Failed to read directory to load data!");

    
    for entry in entries {
        // println!("{:?}", entry);
        let entry= entry.unwrap();

        let name= entry.file_name();
        let name= name.to_string_lossy();

        if name.starts_with("sst_") && name.ends_with(".bin") {
            engine.sstables.load_from_file(&name);
        }
    }
    let mut storage= Storage::new("db.log").expect("Failed to intialize storage!");

    let commands= storage.load().expect("Failed to load database!");

    let mut sstable_id= discover_sstables();

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
                let file= format!("sst_{}.bin", sstable_id);
                sstable_id += 1;

                engine.flush_to_sstable(&file);
                println!("Bye!");
                break;
            }
            _=>{}
        }

        if let Some(output) = engine.execute(command) {
            println!("{}", output);
        }
        
    }
}
