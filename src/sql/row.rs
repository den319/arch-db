use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i64),
    Text(String),
}

#[derive(Debug, PartialEq)]
pub enum RowError {
    ColumnValueCountMismatch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub values: BTreeMap<String, Value>,
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
        value: Value,
    ) {
        self.values.insert(column.into(), value);
    }

    pub fn get(&self, column: &str) -> Option<&Value> {
        self.values.get(column)
    }

    pub fn from_columns(
        columns: Vec<String>,
        values: Vec<Value>,
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
                    Value::Integer(i) => format!("i:{}", i),
                    Value::Text(s) => format!("t:{}", s),
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
                Value::Integer(
                    v.parse().expect("Invalid integer"),
                )
            } else if let Some(v) = value.strip_prefix("t:") {
                Value::Text(v.to_string())
            } else {
                panic!("Unknown value type");
            };

            row.insert(column, value);
        }

        row
    }
}