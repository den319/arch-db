use arch_db::{
    engine::Value,
    error::Result,
    range_iterator::RangeStorageIterator,
    sstable::BlockRecord,
    storage_iterator::StorageIterator,
};

struct MockIterator {
    records: Vec<BlockRecord>,
    pos: usize,
}

impl MockIterator {
    fn new(keys: &[&str]) -> Self {
        Self {
            records: keys
                .iter()
                .map(|k| BlockRecord {
                    key: k.to_string(),
                    value: Value::Data(String::new()),
                })
                .collect(),
            pos: 0,
        }
    }
}

impl StorageIterator for MockIterator {
    fn next(&mut self) -> Result<Option<BlockRecord>> {
        if self.pos >= self.records.len() {
            return Ok(None);
        }

        let record = self.records[self.pos].clone();
        self.pos += 1;

        Ok(Some(record))
    }

    fn peek(&mut self) -> Result<Option<BlockRecord>> {
        if self.pos >= self.records.len() {
            return Ok(None);
        }

        Ok(Some(self.records[self.pos].clone()))
    }
}

#[test]
fn test_empty_range() {
    let iter = MockIterator::new(&[
        "a",
        "b",
        "c",
    ]);

    let mut range =
        RangeStorageIterator::new(
            iter,
            "d".into(),
            "f".into(),
        );

    assert!(range.next().unwrap().is_none());
}

#[test]
fn test_middle_range() {
    let iter = MockIterator::new(&[
        "a",
        "b",
        "c",
        "d",
        "e",
    ]);

    let mut range =
        RangeStorageIterator::new(
            iter,
            "b".into(),
            "e".into(),
        );

    let mut keys = Vec::new();

    while let Some(record) =
        range.next().unwrap()
    {
        keys.push(record.key);
    }

    assert_eq!(
        keys,
        vec!["b", "c", "d"],
    );
}

#[test]
fn test_full_range() {
    let iter = MockIterator::new(&[
        "a",
        "b",
        "c",
    ]);

    let mut range =
        RangeStorageIterator::new(
            iter,
            "a".into(),
            "z".into(),
        );

    let mut keys = Vec::new();

    while let Some(record) =
        range.next().unwrap()
    {
        keys.push(record.key);
    }

    assert_eq!(
        keys,
        vec!["a", "b", "c"],
    );
}

#[test]
fn test_stops_at_end_key() {
    let iter = MockIterator::new(&[
        "a",
        "b",
        "c",
        "d",
        "e",
        "f",
    ]);

    let mut range =
        RangeStorageIterator::new(
            iter,
            "b".into(),
            "d".into(),
        );

    let mut keys = Vec::new();

    while let Some(record) =
        range.next().unwrap()
    {
        keys.push(record.key);
    }

    assert_eq!(
        keys,
        vec!["b", "c"],
    );
}

#[test]
fn test_prefix_scan_range() {
    let iter = MockIterator::new(&[
        "__index__:users:name:Alice:1",
        "__index__:users:name:Alice:2",
        "__index__:users:name:Bob:1",
        "users:1",
    ]);

    let mut end =
        "__index__:users:name:Alice:"
            .to_string();

    end.push(char::MAX);

    let mut range =
        RangeStorageIterator::new(
            iter,
            "__index__:users:name:Alice:"
                .into(),
            end,
        );

    let mut keys = Vec::new();

    while let Some(record) =
        range.next().unwrap()
    {
        keys.push(record.key);
    }

    assert_eq!(
        keys,
        vec![
            "__index__:users:name:Alice:1",
            "__index__:users:name:Alice:2",
        ],
    );
}