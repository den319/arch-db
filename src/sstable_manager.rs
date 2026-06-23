use std::{collections::{BTreeMap, HashMap}, fs::{self, File, OpenOptions}, io::{BufRead, BufReader, Write}, sync::atomic::{AtomicU64, Ordering}};

use crate::{bloom_filter::BloomFilter, engine::Value, error::Result, merge_iterator::MergeIterator, sstable::{SSTableIndex, SSTableIterator, SSTableWriter, load_bloom_from_footer, load_index_from_footer, read_sstable, search_sstable, write_sstable}};


#[derive(Debug, Clone, Copy)]
pub enum Level {
    L0,
    L1,
    L2,
}

#[derive(Debug, Clone, Copy)]
pub enum CompactionStrategy {
    SizeTiered,
    Leveled,
}

#[derive(Debug, Clone)]
pub enum ManifestRecord {

    AddTable {
        level: Level,
        path: String,
        min_key: String,
        max_key: String,
        file_size: u64,
    },

    RemoveTable {
        path: String,
    },
}

pub struct SSTableManager {
    pub l0: Vec<SSTable>,
    pub l1: Vec<SSTable>,
    pub l2: Vec<SSTable>,

    pub strategy: CompactionStrategy,
    pub manifest: Manifest,
}


// #[derive(Debug)]

pub struct SSTable {
    pub path: String,
    pub index: SSTableIndex,
    pub bloom: BloomFilter,
    pub level: Level,

    pub min_key: String,
    pub max_key: String,

    pub file_size: u64,
}

pub struct Manifest {
    log_path: String,
    checkpoint_path: String,
    operations: usize,
}

#[derive(Debug)]
pub struct CreatedTable {
    pub path: String,

    pub index: SSTableIndex,
    pub bloom: BloomFilter,

    pub min_key: String,
    pub max_key: String,

    pub file_size: u64,
}


static SSTABLE_COUNTER: AtomicU64 = AtomicU64::new(0);

const L1_COMPACTION_THRESHOLD: usize = 4;
const SIZE_TIERED_MIN_TABLES: usize = 3;
const SIZE_TIERED_SIZE_RATIO: f64 = 1.5;
const MANIFEST_CHECKPOINT_INTERVAL: usize = 10;
// const TARGET_TABLE_SIZE: usize = 4 * 1024 * 1024;
const TARGET_TABLE_SIZE: usize = 200;


pub fn init_sstable_counter() {
    let mut max_id = 0u64;

    let entries = fs::read_dir(".")
        .expect("Failed to read directory");

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if !name.starts_with("sst_") || !name.ends_with(".bin") {
            continue;
        }

        let stem = name.trim_end_matches(".bin");

        if let Some(id_str) = stem.rsplit('_').next() {
            if let Ok(id) = id_str.parse::<u64>() {
                max_id = max_id.max(id);
            }
        }
    }

    SSTABLE_COUNTER.store(max_id + 1, Ordering::SeqCst);
}

pub fn next_sstable_id() -> u64 {
    SSTABLE_COUNTER.fetch_add(1, Ordering::SeqCst)
}

impl SSTableManager {
    pub fn new() -> Self {
        Self::with_manifest_path("MANIFEST.log")
    }

    pub fn with_manifest_path(path: &str) -> Self {
        Self {
            l0: Vec::new(),
            l1: Vec::new(),
            l2: Vec::new(),
            strategy: CompactionStrategy::Leveled,
            manifest: Manifest::new(path),
        }
    }

    pub fn maybe_compact(&mut self) -> Result<()> {
        match self.strategy {
            CompactionStrategy::Leveled => {
                if self.l0.len() >= 4 {
                    self.size_tiered_compact_l0()?;
                }

                if self.l1.len() >= 4 {
                    self.compact_l1_to_l2()?;
                }
            }

            CompactionStrategy::SizeTiered => {

            }
        }

        Ok(())
    }

    pub fn load_table_metadata(
        &mut self, 
        path: &str,
        level: Level,
        min_key: String,
        max_key: String,
        file_size: u64, 
    ) {

        // Skip if the file no longer exists (e.g. cleaned up by a previous compaction or test)
        if !std::path::Path::new(path).exists() {
            eprintln!("Warning: SSTable not found, skipping: {}", path);
            return;
        }

        let index = match load_index_from_footer(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Warning: failed to load footer {}: {}", path, e);
                return;
            }
        };

        let bloom = match load_bloom_from_footer(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Warning: failed to load bloom footer {}: {}", path, e);
                return;
            }
        };

        let table= SSTable {
            path: path.to_string(),
            index,
            bloom,
            level,
            min_key,
            max_key,
            file_size,
        };

        match level {
            Level::L0 => self.l0.push(table),
            Level::L1 => self.l1.push(table),
            Level::L2 => self.l2.push(table),
        }
    }

    pub fn add_table(&mut self, table: SSTable) {
        // Extract data before moving `table` into the Vec
        let level = table.level;
        let path = table.path.clone();
        let min_key = table.min_key.clone();
        let max_key = table.max_key.clone();
        let file_size = table.file_size;

        self.register_table(table);

        let _= self.manifest.append(
            &ManifestRecord::AddTable { 
                level, 
                path, 
                min_key, 
                max_key, 
                file_size, 
            }
        );
    }

    pub fn register_table(&mut self, table: SSTable) {
        match table.level {
            Level::L0 => self.l0.push(table),
            Level::L1 => self.l1.push(table),
            Level::L2 => self.l2.push(table),
        }
    }

    pub fn find_size_tiered_candidates(&self) -> Vec<usize> {
        if self.l0.len() < SIZE_TIERED_MIN_TABLES {
            return vec![];
        }

        let mut indexed: Vec<(usize, u64)> = self.l0
            .iter()
            .enumerate()
            .map(|(i, t)| (i, t.file_size))
            .collect();

        indexed.sort_by_key(|(_, size)| *size);

        for window in indexed.windows(SIZE_TIERED_MIN_TABLES) {
            let min = window.first().unwrap().1 as f64;
            let max = window.last().unwrap().1 as f64;

            if max / min <= SIZE_TIERED_SIZE_RATIO {
                return window.iter().map(|(idx, _)| *idx).collect();
            }
        }

        vec![]
    }

    pub fn get(&self, key:&str) -> Result<Option<Value>> {
        for table in self.l0.iter().rev() {
            if !table.bloom.contains(&key) {
                continue;
            }

            if let Some((_, value)) =
                search_sstable(&table.path, &table.index, key)?
            {
                return Ok(Some(value));
            }
        }

        // L1 newest -> oldest
        for table in self.l1.iter().rev() {
            if !table.bloom.contains(&key) {
                continue;
            }

            if let Some((_, value)) =
                search_sstable(&table.path, &table.index, key)?
            {
                return Ok(Some(value));
            }
        }

        // L2 newest -> oldest
        for table in self.l2.iter().rev() {
            if !table.bloom.contains(&key) {
                continue;
            }

            if let Some((_, value)) =
                search_sstable(&table.path, &table.index, key)?
            {
                return Ok(Some(value));
            }
        }

        Ok(None)
    }

    fn finalize_writer(
        &self,
        mut writer: SSTableWriter,
        path: String,
        min_key: String,
        max_key: String,
    ) -> Result<CreatedTable> {

        let index = writer.finish()?;

        let bloom = load_bloom_from_footer(&path)?;

        let file_size = fs::metadata(&path)?.len();

        Ok(CreatedTable {
            path,
            index,
            bloom,
            min_key,
            max_key,
            file_size,
        })
    }

    fn write_merged_tables(
        &self,
        tables: &[&SSTable],
        output_path: &str,
        output_level: Level,
        drop_tombstones: bool,
    ) -> Result<Vec<CreatedTable>> {
        let mut iters = Vec::new();

        for table in tables {
            iters.push(
                SSTableIterator::new(
                    &table.path,
                    table.index.clone(),
                )?
            );
        }

        let mut merge =
            MergeIterator::new(
                iters,
                drop_tombstones,
            )?;
        
        let estimated_records: usize = tables
            .iter()
            .map(|t| t.index.offsets.len())
            .sum();

        let mut created_tables = Vec::new();

        let mut current_path = format!(
            "sst_{:?}_{}.bin",
            output_level,
            next_sstable_id(),
        ).to_lowercase();

        let mut writer = SSTableWriter::new(
            &current_path,
            TARGET_TABLE_SIZE,
        )?;

        let mut current_min_key: Option<String> = None;
        let mut current_max_key: Option<String> = None;

        let mut estimated_size = 0usize;

        while let Some(record) = merge.next()? {

            if current_min_key.is_none() {
                current_min_key = Some(record.key.clone());
            }

            current_max_key = Some(record.key.clone());

            let record_size = match &record.value {
                Value::Data(v) => {
                    record.key.len()
                        + v.len()
                        + 32 // overhead approximation
                }
                Value::Tombstone => {
                    record.key.len()
                        + 16
                }
            };

            writer.append(
                record.key,
                record.value,
            )?;

            estimated_size += record_size;

            if estimated_size >= TARGET_TABLE_SIZE {

                let table = self.finalize_writer(
                    writer,
                    current_path.clone(),
                    current_min_key.take().unwrap(),
                    current_max_key.take().unwrap(),
                )?;

                created_tables.push(table);

                current_path = format!(
                    "sst_{:?}_{}.bin",
                    output_level,
                    next_sstable_id(),
                )
                .to_lowercase();

                writer = SSTableWriter::new(
                    &current_path,
                    TARGET_TABLE_SIZE,
                )?;

                estimated_size = 0;
            }
        }

        if estimated_size > 0 {

            let table = self.finalize_writer(
                writer,
                current_path,
                current_min_key.unwrap(),
                current_max_key.unwrap(),
            )?;

            created_tables.push(table);
        }

        Ok(created_tables)
    }

    fn all_tables(&self) -> impl Iterator<Item = &SSTable> {
        self.l0
            .iter()
            .chain(self.l1.iter())
            .chain(self.l2.iter())
    }

    pub fn compact(&mut self) -> Result<()> {
        let mut merged= HashMap::new();


        for table in self.all_tables() {
            let data= read_sstable(&table.path)?;

            for (key, val) in data {
                match val {
                    Value::Data(v) => {
                        merged.insert(key, Value::Data(v));
                    }
                    Value::Tombstone => {
                        merged.remove(&key);
                    }
                }
            }
        }

        let mut sorted:Vec<(String, Value)>= merged.into_iter().collect();

        sorted.sort_by(|a,b| a.0.cmp(&b.0));

        let min_key= sorted.first().map(|(k, _)|k.clone()).unwrap();

        let max_key= sorted.last().map(|(k, _)|k.clone()).unwrap();


        let table_paths: Vec<String> = self.all_tables().map(|t| t.path.clone()).collect();

        for path in &table_paths {
            let _ = fs::remove_file(path)?;
            self.manifest.append(&ManifestRecord::RemoveTable { path: path.clone() })?;
        }

        self.l0.clear();
        self.l1.clear();
        self.l2.clear();

        
        if sorted.is_empty() {
            return Ok(());
        }

        let mut bloom = BloomFilter::with_rate(0.01, sorted.len() as u32);

        for (key, _) in &sorted {
            bloom.insert(key);
        }

        let path= format!("sst_l1_{}.bin", next_sstable_id());

        let index= write_sstable(&path, &sorted)?;

        let file_size = fs::metadata(&path)?.len();

        self.l1.push(SSTable { path, index, bloom, level: Level::L1, min_key, max_key, file_size });

        self.checkpoint_manifest()?;

        Ok(())
    }

    pub fn compact_l1_to_l2(&mut self) -> Result<()> {
        if self.l1.is_empty() {
            return Ok(());
        }

        let l1_min = self.l1[0].min_key.clone();
        let l1_max = self.l1[0].max_key.clone();

        let overlapping_indices:Vec<usize> = self.l2.iter().enumerate()
            .filter(|(_,table)| {
                table.overlaps(&l1_min, &l1_max)
            })
            .map(|(idx, _)| idx)
            .collect();

        println!("Found {} overlapping tables",overlapping_indices.len());

        let mut tables: Vec<&SSTable> = Vec::new();

        // Older L2 tables first
        for idx in &overlapping_indices {
            tables.push(&self.l2[*idx]);
        }

        // Newer L1 table last
        tables.push(&self.l1[0]);

        let path = format!(
            "sst_l2_{}.bin",
            next_sstable_id()
        );

        let created_tables = self.write_merged_tables(
            &tables,
            &path,
            Level::L2,
            true,
        )?;


        for idx in overlapping_indices.iter().rev() {
            let table= self.l2.remove(*idx);

            self.manifest.append(&ManifestRecord::RemoveTable { path: table.path.clone() })?;
            fs::remove_file(&table.path)?;
        }

        self.manifest.append(&ManifestRecord::RemoveTable { path: self.l1[0].path.clone() })?;
        fs::remove_file(&self.l1[0].path)?;

        self.l1.remove(0);

        for table in created_tables {
            self.install_table(
                Level::L2,
                table.path,
                table.index,
                table.bloom,
                table.min_key,
                table.max_key,
                table.file_size,
            )?;

        }

        self.checkpoint_manifest()?;

        Ok(())
    }

    pub fn size_tiered_compact_l0(&mut self) -> Result<()> {
        let candidates = self.find_size_tiered_candidates();

        if candidates.is_empty() {
            return Ok(());
        }

        let mut indexed_candidates = candidates.clone();
        indexed_candidates.sort();

        // L0: higher index = newer data. Reverse so newer values overwrite older ones.
        let tables: Vec<&SSTable> = indexed_candidates
            .iter()
            .map(|idx| &self.l0[*idx])
            .collect();
        // tables.reverse();

        let path = format!(
            "sst_l1_{}.bin",
            next_sstable_id()
        );

        let created_tables= self.write_merged_tables(
            &tables,
            &path,
            Level::L1,
            false,
        )?;


        // Remove candidates in descending index order so that
        // removing one doesn't shift the indices of remaining candidates.
        let mut sorted_candidates = candidates.clone();
        sorted_candidates.sort_by(|a, b| b.cmp(a));
        for idx in sorted_candidates {
            let old = self.l0.remove(idx);

            self.manifest.append(&ManifestRecord::RemoveTable { path: old.path.clone() })?;
            fs::remove_file(old.path)?;
        }

        for table in created_tables {
            self.install_table(
                Level::L1,
                table.path,
                table.index,
                table.bloom,
                table.min_key,
                table.max_key,
                table.file_size,
            )?;

        }

        Ok(())
    }

    pub fn write_manifest_checkpoint(&self) -> Result<()> {
        let tables: Vec<&SSTable> = self
            .l0
            .iter()
            .chain(self.l1.iter())
            .chain(self.l2.iter())
            .collect();

        self.manifest.write_checkpoint(&tables)
    }

    pub fn checkpoint_manifest(&mut self) -> Result<()> {
        self.write_manifest_checkpoint()?;

        self.manifest.clear_log()?;

        Ok(())
    }

    fn install_table(
        &mut self,
        level: Level,
        path: String,
        index: SSTableIndex,
        bloom: BloomFilter,
        min_key: String,
        max_key: String,
        file_size: u64,
    ) -> Result<()> {
        let table = SSTable {
            path,
            index,
            bloom,
            level,
            min_key,
            max_key,
            file_size,
        };

        self.add_table(table);

        if self.manifest.should_checkpoint() {
            self.checkpoint_manifest()?;
        }

        Ok(())
    }


}

impl ManifestRecord {
    pub fn serialize(&self) -> String {
        match self {
            ManifestRecord::AddTable { level, path, min_key, max_key, file_size } => {
                format!(
                    "ADD|{:?}|{}|{}|{}|{}\n",
                    level, path, min_key, max_key, file_size,
                )
            }
            ManifestRecord::RemoveTable { path } => {
                format!("REMOVE|{}\n", path)
            }
        }
    }

    pub fn deserialize(line: &str) -> Option<Self> {
        let parts: Vec<&str>= line.trim().split('|').collect();

        match parts.first()? {
            &"ADD" => {
                if parts.len() != 6 {
                    return None;
                }

                let level = match parts[1] {
                    "L0" => Level::L0,
                    "L1" => Level::L1,
                    "L2" => Level::L2,

                    _ => return None,
                };

                Some(ManifestRecord::AddTable { 
                    level, 
                    path: parts[2].to_string(), 
                    min_key: parts[3].to_string(), 
                    max_key: parts[4].to_string(), 
                    file_size: parts[5].parse().ok()?,
                })
            }
            &"REMOVE" => {
                if parts.len() != 2 {
                    return None;
                }

                Some(ManifestRecord::RemoveTable { path: parts[1].to_string() })
            }

            _ => None,
        }
    }
}

impl Manifest {
    pub fn new(path: &str) -> Self {
        Self {
            log_path: path.to_string(),
            checkpoint_path: "MANIFEST.checkpoint".to_string(),
            operations: 0,
        }
    }

    pub fn set_checkpoint_path(&mut self, path: &str) {
        self.checkpoint_path = path.to_string();
    }

    pub fn clear_log(&mut self) -> Result<()> {
        File::create(&self.log_path)?;
        self.operations= 0;
        Ok(())
    }

    pub fn append(&mut self, record: &ManifestRecord) -> Result<()> {
        
        let mut file= OpenOptions::new().create(true).append(true).open(&self.log_path)?;

        file.write_all(record.serialize().as_bytes())?;

        file.sync_all()?;
        self.operations += 1;

        Ok(())
    }

    pub fn load_log(&self) -> Result<Vec<ManifestRecord>> {
        if !std::path::Path::new(&self.log_path).exists() {
            return Ok(vec![]);
        }

        let file= File::open(&self.log_path)?;
        

        let reader= BufReader::new(file);

        let mut records= vec![];

        for line in reader.lines() {
            let line= line?;

            if let Some(record)= ManifestRecord::deserialize(&line) {
                records.push(record);
            }
        }

        Ok(records)
    }

    pub fn load_checkpoint(&self) -> Result<Vec<ManifestRecord>> {
        if !std::path::Path::new(&self.checkpoint_path).exists() {
            return Ok(vec![]);
        }

        let file = File::open(&self.checkpoint_path)?;
        let reader = BufReader::new(file);

        let mut records = vec![];

        for line in reader.lines() {
            let line = line?;

            if let Some(record) = ManifestRecord::deserialize(&line) {
                records.push(record);
            }
        }

        Ok(records)
    }

    pub fn write_checkpoint(
        &self,
        tables: &[&SSTable],
    ) -> Result<()> {

        let mut file = File::create(&self.checkpoint_path)?;
        for table in tables {
            let record = ManifestRecord::AddTable {
                level: table.level,
                path: table.path.clone(),
                min_key: table.min_key.clone(),
                max_key: table.max_key.clone(),
                file_size: table.file_size,
            };
            file.write_all(record.serialize().as_bytes())?;
        }   

        file.sync_all()?;
        Ok(())
    }

    pub fn should_checkpoint(&self) -> bool {
        self.operations >= MANIFEST_CHECKPOINT_INTERVAL
    }


}



