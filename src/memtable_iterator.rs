use std::collections::BTreeMap;

use crate::{
    engine::Value, error::Result, sstable::BlockRecord, storage_iterator::StorageIterator,
};

pub struct MemtableIterator {
    records: Vec<BlockRecord>,
    position: usize,
}

impl MemtableIterator {
    pub fn new(memtable: &BTreeMap<String, Value>) -> Self {

        let records = memtable
            .iter()
            .map(|(key, value)| BlockRecord {
                key: key.clone(),
                value: value.clone(),
            })
            .collect();
        Self {
            records,
            position: 0,
        }
    }

    pub fn from_range<I>(iter: I) -> Self
    where
        I: Iterator<Item = (String, Value)>,
    {
        let entries = iter
            .map(|(key, value)| BlockRecord { key, value })
            .collect();

        Self {
            records: entries,
            position: 0,
        }
    }
}

impl StorageIterator for MemtableIterator {
    fn next(&mut self) -> Result<Option<BlockRecord>> {
        let record = self.peek()?;

        if record.is_some() {
            self.position += 1;
        }

        Ok(record)
    }

    fn peek(&mut self) -> Result<Option<BlockRecord>> {
        Ok(
            self.records
                .get(self.position)
                .cloned()
        )
    }
}
