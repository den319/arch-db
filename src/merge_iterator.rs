use std::{cmp::Ordering, collections::BinaryHeap};

use crate::{engine::Value, error::Result, sstable::{BlockRecord, SSTableIterator}};

pub struct MergeIterator {
    iters: Vec<SSTableIterator>,
    heap: BinaryHeap<HeapItem>,
    drop_tombstones: bool,
}

#[derive(Clone, Debug)]
pub struct HeapItem {
    key: String,
    value: Value,
    iter_index: usize,
}

impl Eq for HeapItem {}

impl PartialEq for HeapItem {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Ord for HeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        match other.key.cmp(&self.key) {
            Ordering::Equal => {
                other.iter_index.cmp(&self.iter_index)
            }
            ord => ord,
        }
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

        let item = match self.heap.pop() {
            Some(item) => item,
            None => return Ok(None),
        };

        let returned_key= item.key.clone();

        if let Some(record)= self.iters[item.iter_index].next()? {
            self.heap.push(HeapItem {
                key: record.key,
                value: record.value,
                iter_index: item.iter_index,
            });
        }
        while let  Some(top) = self.heap.peek() {
            if top.key != returned_key {
                break;
            }
            let duplicate= self.heap.pop().unwrap();

            if let Some(record) = self.iters[duplicate.iter_index].next()? {
    
                self.heap.push(HeapItem {
                    key: record.key,
                    value: record.value,
                    iter_index: duplicate.iter_index,
                });
            }
            // if self.drop_tombstones {
            //     if let Value::Tombstone = item.value {
            //         continue;
            //     }
            // }
        } 


        return Ok(Some(BlockRecord {
            key: item.key,
            value: item.value,
        }));
    }
}