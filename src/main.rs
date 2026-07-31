use std::{collections::HashSet, fs, io::{self, Write}};

use arch_db::{command::Command, engine::Engine, parser::parse, sstable::{load_bloom_from_footer, load_index_from_footer}, sstable_manager::{Level, ManifestRecord, SSTable, init_sstable_counter}};
use arch_db::sql::{catalog::Catalog, executor::Executor, lexer::Lexer, sql_parser::SQLParser};

fn main() {
    init_sstable_counter();

    let mut engine = Engine::new();

    let mut catalog = Catalog::new();

    // Replay manifest to restore known SSTables
    // Uses load_table_metadata with manifest's stored min_key/max_key/file_size
    // instead of scanning the full SSTable data — O(number of SSTables), not O(total data)
    {
        let mut sstables = engine.sstables.lock().unwrap();

        // ------------------------------------------------ implementation-1
        
        // let checkpoint= sstables.manifest.load_checkpoint().expect("Failed to load checkpoint");

        // for entry in checkpoint {
        //     match entry {
        //         ManifestRecord::AddTable { level, path, min_key, max_key, file_size } => {
        //             sstables.load_table_metadata(
        //                     &path,
        //                     level,
        //                     min_key,
        //                     max_key,
        //                     file_size,
        //             );
                                        
        //         }
        //         ManifestRecord::RemoveTable { path } => {
        //             let _= path;
        //         }
        //     }
        // }
        // let logs = sstables.manifest.load_log().expect("Failed to load manifest");

        // for entry in logs {
        //     match entry {
        //         ManifestRecord::AddTable { level, path, min_key, max_key, file_size } => {
        //             sstables.load_table_metadata(
        //                 &path,
        //                 level,
        //                 min_key,
        //                 max_key,
        //                 file_size,
        //             );
        //         }
        //         ManifestRecord::RemoveTable { path } => {
        //             // Already handled; skip removed tables
        //             let _ = path;
        //         }
        //     }
        // }


        // ------------------------------------------------ implementation-2

        // Load checkpoint entries
        if let Ok(checkpoint) = sstables.manifest.load_checkpoint() {
            for entry in checkpoint {
                match entry {
                    ManifestRecord::AddTable { level, path, min_key, max_key, file_size } => {
                        sstables.load_table_metadata(&path, level, min_key, max_key, file_size);
                    }
                    ManifestRecord::RemoveTable { .. } => {}
                }
            }
        }

        // Load log entries
        if let Ok(logs) = sstables.manifest.load_log() {
            for entry in logs {
                match entry {
                    ManifestRecord::AddTable { level, path, min_key, max_key, file_size } => {
                        sstables.load_table_metadata(&path, level, min_key, max_key, file_size);
                    }
                    ManifestRecord::RemoveTable { .. } => {}
                }
            }
        }

        // Collect paths already known from the manifest
        let known_paths: HashSet<String> = sstables
            .l0
            .iter()
            .chain(sstables.l1.iter())
            .chain(sstables.l2.iter())
            .map(|t| t.path.clone())
            .collect();

        // Scan for orphaned SSTables not in the manifest
        if let Ok(entries) = fs::read_dir(".") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();

                if !name.starts_with("sst_") || !name.ends_with(".bin") {
                    continue;
                }

                if known_paths.contains(&name) {
                    continue;
                }

                // This is an orphaned SSTable — load it as L0
                let index = match load_index_from_footer(&name) {
                    Ok(idx) => idx,
                    Err(_) => continue,
                };
                let bloom = match load_bloom_from_footer(&name) {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                let file_size = match fs::metadata(&name) {
                    Ok(m) => m.len(),
                    Err(_) => continue,
                };
                let min_key = index.offsets.keys().next().cloned().unwrap_or_default();
                let max_key = index.offsets.keys().next_back().cloned().unwrap_or_default();

                sstables.l0.push(SSTable {
                    path: name,
                    index,
                    bloom,
                    level: Level::L0,
                    min_key,
                    max_key,
                    file_size,
                });
            }
        }
    
    }

    if let Err(err) = catalog.load_from_engine(&mut engine) {
        println!("Failed to load catalog: {}", err);
        return;
    }

    if let Err(err) = catalog.load_indexes_from_engine(&mut engine) {
        println!("Failed to load indexes: {}", err);
        return;
    }

    loop {
        print!("archdb > ");
        io::stdout().flush().unwrap();
        let mut input = String::new();

        io::stdin().read_line(&mut input).expect("Failed to read line");

        if is_sql(&input) {
            let lexer = Lexer::new(&input.trim());
            let mut parser = SQLParser::new(lexer);
            let statement = parser.parse_statement();
            let mut executor = Executor::new(&mut catalog, &mut engine);
            let result = executor.execute(statement);

            match result {
                arch_db::sql::executor::QueryResult::None => {}
                arch_db::sql::executor::QueryResult::Message(msg) => {
                    println!("{}", msg);
                }
                arch_db::sql::executor::QueryResult::Rows(rows) => {
                    for row in rows {
                        println!("{}", row.join(" | "));
                    }
                }
            }

            continue;
        }

        let command = parse(&input);

        match &command {
            Command::Exit => {
                if let Err(err) = engine.shutdown() {
                    println!("Shutdown failed: {}", err);
                }
                println!("Bye!");
                break;
            }
            _ => {}
        }

        if let Some(output) = engine.execute(command) {
            println!("{}", output);
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