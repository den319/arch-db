mod command;
mod engine;
mod parser;
mod storage;
mod error;
mod sstable;

use std::{io::{self, Write}};

use engine::Engine;
use parser::parse;
use command::Command;
use storage::Storage;

fn main() {
    let mut engine= Engine::new();

    let mut storage= Storage::new("db.log").expect("Failed to intialize storage!");

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
                engine.flush_to_sstable("data.sst");
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
