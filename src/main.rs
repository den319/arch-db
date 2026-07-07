use std::{io::{self, Write}};

use arch_db::{command::Command, engine::Engine, parser::parse, sql::{catalog::Catalog, executor::Executor, lexer::Lexer, sql_parser::SQLParser}, sstable_manager::{ManifestRecord, init_sstable_counter, next_sstable_id}, storage::{self, Storage}};


fn main() {
    init_sstable_counter();

    let mut engine = Engine::new();

    let mut catalog = Catalog::new();

    // Replay manifest to restore known SSTables
    // Uses load_table_metadata with manifest's stored min_key/max_key/file_size
    // instead of scanning the full SSTable data — O(number of SSTables), not O(total data)
    {
        let mut sstables = engine.sstables.lock().unwrap();
        let checkpoint= sstables.manifest.load_checkpoint().expect("Failed to load checkpoint");

        for entry in checkpoint {
            match entry {
                ManifestRecord::AddTable { level, path, min_key, max_key, file_size } => {
                    sstables.load_table_metadata(
                            &path,
                            level,
                            min_key,
                            max_key,
                            file_size,
                    );
                                        
                }
                ManifestRecord::RemoveTable { path } => {
                    let _= path;
                }
            }
        }
        let logs = sstables.manifest.load_log().expect("Failed to load manifest");

        for entry in logs {
            match entry {
                ManifestRecord::AddTable { level, path, min_key, max_key, file_size } => {
                        sstables.load_table_metadata(
                            &path,
                            level,
                            min_key,
                            max_key,
                            file_size,
                        );
                }
                ManifestRecord::RemoveTable { path } => {
                    // Already handled; skip removed tables
                    let _ = path;
                }
            }
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

        if is_sql(&input) {

            let lexer = Lexer::new(&input.trim());

            let mut parser = SQLParser::new(lexer);

            let statement = parser.parse_statement();

            let mut executor = Executor::new(
                &mut catalog,
                &mut engine,
            );

            let result = executor.execute(statement);

            println!("{:#?}", result);

            continue;
        }

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


fn is_sql(input: &str) -> bool {
    let upper = input.trim().to_ascii_uppercase();

    upper.starts_with("SELECT")
        || upper.starts_with("INSERT")
        || upper.starts_with("UPDATE")
        || upper.starts_with("DELETE")
        || upper.starts_with("CREATE")
}