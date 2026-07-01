
use crate::{
    error::Result, sstable::{BlockRecord, SSTableIterator},
};

pub trait StorageIterator {
    fn next(&mut self) -> Result<Option<BlockRecord>>;
    // fn seek(&mut self, key: &str) -> Result<()>;
    fn peek(&mut self) -> Result<Option<BlockRecord>>;
}

impl StorageIterator for SSTableIterator {
    fn next(&mut self) -> Result<Option<BlockRecord>> {
        let record = self.peek()?;

        if record.is_some() {
            self.current_record += 1;
        }

        Ok(record)
    }

    fn peek(&mut self) -> Result<Option<BlockRecord>> {
        loop {
            if self.current_record < self.block_records.len() {
                return Ok(Some(
                    self.block_records[self.current_record].clone()
                ));
            }

            if !self.load_next_block()? {
                return Ok(None);
            }
        }
    }
}