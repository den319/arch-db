use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::{
    engine::Value, error::Result, storage_iterator::StorageIterator,
};

pub struct UnifiedStorageIterator {
    iters: Vec<Box<dyn StorageIterator>>,
    heap: BinaryHeap<HeapItem>,
}