use crate::{
    error::Result,
    merge_iterator::MergeIterator,
    sstable::BlockRecord,
};

pub struct EngineIterator {
    merge: MergeIterator,
}

impl EngineIterator {
    pub fn new(merge: MergeIterator) -> Self {
        Self { merge }
    }

    pub fn next(
        &mut self,
    ) -> Result<Option<BlockRecord>> {
        self.merge.next()
    }
}