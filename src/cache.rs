use std::collections::{HashMap, VecDeque};

use crate::{engine::Value, sstable::BlockRecord};

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct CacheKey {
    pub path: String,
    pub offset: u64,
}

pub struct BlockCache {
    capacity: usize,
    map: HashMap<CacheKey, Vec<BlockRecord>>,
    usage: VecDeque<CacheKey>,
}

impl BlockCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            map: HashMap::new(),
            usage: VecDeque::new(),
        }
    }
    
    pub fn get(&mut self, key: &CacheKey) -> Option<Vec<BlockRecord>> {
        if let Some(value)= self.map.get(key) {
            self.usage.retain(|k| k != key);

            self.usage.push_back(key.clone());

            return Some(value.clone());
        }

        None
    }

    pub fn insert(&mut self, key: CacheKey, value: Vec<BlockRecord>) {
        if self.map.contains_key(&key) {
            self.usage.retain(|k| k!= &key);
        }

        self.usage.push_back(key.clone());

        self.map.insert(key.clone(), value);

        if self.map.len() > self.capacity {
            if let Some(oldest)= self.usage.pop_front() {
                self.map.remove(&oldest);
            }
        }
    }

    
}