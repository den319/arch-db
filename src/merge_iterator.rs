use std::{cmp::Ordering, collections::BinaryHeap};

use crate::{engine::Value, error::Result, sstable::{BlockRecord, SSTableIterator}};

pub struct MergeIterator {
    iters: Vec<SSTableIterator>,
    heap: BinaryHeap<HeapItem>,
    drop_tombstones: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeapItem {
    key: String,
    value: Value,
    iter_index: usize,
}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .key
            .cmp(&self.key)
            // Higher iter_index = newer data = wins for duplicate keys
            .then_with(|| self.iter_index.cmp(&other.iter_index))
    }
}

impl PartialOrd for HeapItem {
    fn partial_cmp(
        &self,
        other: &Self,
    ) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl MergeIterator {
    pub fn new(mut iters: Vec<SSTableIterator>, drop_tombstones: bool) -> Result<Self> {
        let mut heap= BinaryHeap::new();

        for(iter_index, iter) in iters.iter_mut().enumerate() {
            if let Some(record)= iter.next()? {
                println!(
                    "PUSH key={} iter={}",
                    record.key,
                    iter_index
                );
                heap.push(HeapItem { key: record.key, value: record.value, iter_index });
            }
        }
        
        Ok(Self {
            iters, 
            heap,
            drop_tombstones
        })
    }

    pub fn next(
        &mut self,
    ) -> Result<Option<BlockRecord>> {

        // Pop the winning record (newest version for the smallest key)
        let item = match self.heap.pop() {
            Some(item) => item,
            None => return Ok(None),
        };

        // Save the key because we'll compare against duplicates.
        let returned_key = item.key.clone();

        // ------------------------------------------------------
        // Step 1: Remove every duplicate for this key.
        // ------------------------------------------------------

        while let Some(top) = self.heap.peek() {
            if top.key != returned_key {
                break;
            }

            let duplicate = self.heap.pop().unwrap();

            // Advance the duplicate iterator.
            if let Some(record) = self.iters[duplicate.iter_index].next()? {
                self.heap.push(HeapItem {
                    key: record.key,
                    value: record.value,
                    iter_index: duplicate.iter_index,
                });
            }
        }

        // ------------------------------------------------------
        // Step 2: Advance the winning iterator.
        // ------------------------------------------------------

        if let Some(record) = self.iters[item.iter_index].next()? {
            self.heap.push(HeapItem {
                key: record.key,
                value: record.value,
                iter_index: item.iter_index,
            });
        }

        // ------------------------------------------------------
        // Step 3: Optionally drop tombstones.
        // ------------------------------------------------------

        if self.drop_tombstones {
            if let Value::Tombstone = item.value {
                // Skip this key completely.
                return self.next();
            }
        }

        Ok(Some(BlockRecord {
            key: item.key,
            value: item.value,
        }))
    }
}