use std::{collections::BTreeMap, fmt};

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
)]
pub enum RowValue {
    Integer(i64),
    Text(String),
}

#[derive(Debug, PartialEq)]
pub enum RowError {
    ColumnValueCountMismatch,
    UnknownColumn,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub values: BTreeMap<String, RowValue>,
}


impl Row {
    pub fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    pub fn insert(
        &mut self,
        column: impl Into<String>,
        value: RowValue,
    ) {
        self.values.insert(column.into(), value);
    }

    pub fn get(&self, column: &str) -> Option<&RowValue> {
        self.values.get(column)
    }

    pub fn update(
        &mut self,
        column: &str,
        value: RowValue,
    ) -> Result<(), RowError> {
        match self.values.get_mut(column) {
            Some(existing) => {
                *existing = value;
                Ok(())
            }

            None => Err(RowError::UnknownColumn),
        }
    }

    pub fn from_columns(
        columns: Vec<String>,
        values: Vec<RowValue>,
    ) -> Result<Self, RowError> {

        if columns.len() != values.len() {
            return Err(RowError::ColumnValueCountMismatch);
        }

        let mut row = Row::new();

        for (column, value) in columns.into_iter().zip(values.into_iter()) {
            row.insert(column, value);
        }

        Ok(row)
    }

    pub fn serialize(&self) -> String {
        self.values
            .iter()
            .map(|(column, value)| {
                let value = match value {
                    RowValue::Integer(i) => format!("i:{}", i),
                    RowValue::Text(s) => format!("t:{}", s),
                };

                format!("{}={}", column, value)
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    pub fn deserialize(input: &str) -> Self {
        let mut row = Row::new();

        if input.is_empty() {
            return row;
        }

        for pair in input.split('|') {
            let (column, value) = pair
                .split_once('=')
                .expect("Invalid row format");

            let value = if let Some(v) = value.strip_prefix("i:") {
                RowValue::Integer(
                    v.parse().expect("Invalid integer"),
                )
            } else if let Some(v) = value.strip_prefix("t:") {
                RowValue::Text(v.to_string())
            } else {
                panic!("Unknown value type");
            };

            row.insert(column, value);
        }

        row
    }
}

impl RowValue {
    pub fn as_storage_string(&self) -> String {
        match self {
            RowValue::Integer(i) => i.to_string(),
            RowValue::Text(s) => s.clone(),
        }
    }

    pub fn as_integer(
        &self,
    ) -> Option<i64> {

        match self {

            RowValue::Integer(value) => Some(*value),

            RowValue::Text(_) => {
                None
            },
            
            _ => None,
        }
    }
}



impl fmt::Display for RowValue {

    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {

        match self {

            RowValue::Integer(value) => {
                return write!(f, "{}", value);
            }

            RowValue::Text(value) => {
                return write!(f, "{}", value);
            }
        }
    }
}