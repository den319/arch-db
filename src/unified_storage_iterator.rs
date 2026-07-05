use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::{
    error::Result, sstable::BlockRecord, storage_iterator::StorageIterator,
};


#[derive(Debug, Clone)]
struct HeapItemForUnifiedIter {
    record: BlockRecord,

    // Which iterator produced this record
    iter_index: usize,
}
pub struct UnifiedStorageIterator {
    iters: Vec<Box<dyn StorageIterator>>,
    heap: BinaryHeap<HeapItemForUnifiedIter>,
}

impl Eq for HeapItemForUnifiedIter {}

impl PartialEq for HeapItemForUnifiedIter {
    fn eq(&self, other: &Self) -> bool {
        self.record.key == other.record.key
    }
}

impl Ord for HeapItemForUnifiedIter {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .record
            .key
            .cmp(&self.record.key)
            .then_with(|| other.iter_index.cmp(&self.iter_index))
    }
}

impl PartialOrd for HeapItemForUnifiedIter {
    fn partial_cmp(
        &self,
        other: &Self,
    ) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl UnifiedStorageIterator {
    pub fn new(
        mut iters: Vec<Box<dyn StorageIterator>>,
    ) -> Result<Self> {

        let mut heap = BinaryHeap::new();

        for (idx, iter) in iters.iter_mut().enumerate() {
            if let Some(record) = iter.next()? {
                heap.push(HeapItemForUnifiedIter {
                    record,
                    iter_index: idx,
                });
            }
        }

        Ok(Self {
            iters,
            heap,
        })
    }
}

impl StorageIterator for UnifiedStorageIterator {
    fn next(
        &mut self,
    ) -> Result<Option<BlockRecord>> {

        // Nothing left
        let item = match self.heap.pop() {
            Some(item) => item,
            None => return Ok(None),
        };

        let key = item.record.key.clone();

        //
        // Advance the iterator that produced the winning record.
        //
        if let Some(record) = self.iters[item.iter_index].next()? {
            self.heap.push(HeapItemForUnifiedIter {
                record,
                iter_index: item.iter_index,
            });
        }

        //
        // Remove every duplicate of the same key.
        //
        while let Some(top) = self.heap.peek() {

            if top.record.key != key {
                break;
            }

            let duplicate = self.heap.pop().unwrap();

            // Advance that iterator too.
            if let Some(record) =
                self.iters[duplicate.iter_index].next()?
            {
                self.heap.push(HeapItemForUnifiedIter {
                    record,
                    iter_index: duplicate.iter_index,
                });
            }
        }

        Ok(Some(item.record))
    }

    fn peek(
        &mut self,
    ) -> Result<Option<BlockRecord>> {
        todo!()
    }
}