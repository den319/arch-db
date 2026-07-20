use crate::{
    error::Result, sstable::BlockRecord, storage_iterator::StorageIterator,
};

pub struct RangeStorageIterator<I>
where
    I: StorageIterator,
{
    inner: I,
    start: String,
    end: String,
}

impl<I> RangeStorageIterator<I>
where
    I: StorageIterator,
{
    pub fn new(
        inner: I,
        start: String,
        end: String,
    ) -> Self {
        Self {
            inner,
            start,
            end,
        }
    }
}

impl<I> StorageIterator for RangeStorageIterator<I>
where
    I: StorageIterator,
{
    fn next(
        &mut self,
    ) -> Result<Option<BlockRecord>> {

        while let Some(record) =
            self.inner.next()?
        {
            if record.key < self.start {
                continue;
            }

            if record.key >= self.end {
                return Ok(None);
            }

            return Ok(Some(record));
        }

        Ok(None)
    }

    fn peek(
        &mut self,
    ) -> Result<Option<BlockRecord>> {

        // TODO:
        // peek() is not fully correct for range iterators.
        // It returns None while positioned before the lower bound,
        // even though next() would eventually return an in-range record.
        // Implement buffered skipping if range iterators are later used
        // by merge iterators or query planning.

        

        // Peek the inner iterator and check if the record is in range.
        // We need to skip out-of-range records without advancing the inner
        // iterator permanently, but peek is supposed to be non-destructive.
        // For simplicity, we peek the inner and apply range filtering.
        // If the peeked record is out of range, we return None.
        match self.inner.peek()? {
            Some(record) => {
                if record.key < self.start {
                    // The inner iterator is positioned before start.
                    // We can't skip without advancing, so return None
                    // and let next() handle the skipping.
                    return Ok(None);
                }

                if record.key >= self.end {
                    return Ok(None);
                }

                Ok(Some(record))
            }

            None => Ok(None),
        }
    }
}
