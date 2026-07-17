use std::collections::HashMap;

use crate::{engine::{Engine, Value}, error::{DatabaseError, Result}, sql::ast::{self, DataType}, storage_iterator::StorageIterator};

#[derive(Debug, Clone, PartialEq)]
pub enum CatalogDataType {
    Integer,
    Text,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub name: String,
    pub data_type: CatalogDataType,
    pub primary_key: bool,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableSchema {
    pub name: String,
    pub columns: Vec<Column>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSchema {
    pub name: String,
    pub table_name: String,
    pub column_name: String,
}

pub struct Catalog {
    pub tables: HashMap<String, TableSchema>,
    pub indexes: HashMap<String, IndexSchema>,
}

impl From<ast::DataType> for CatalogDataType {
    fn from(value: ast::DataType) -> Self {
        match value {
            ast::DataType::Int => CatalogDataType::Integer,
            ast::DataType::Text => CatalogDataType::Text,
        }
    }
}

impl Catalog {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            indexes: HashMap::new(),
        }
    }

    pub fn create_table(
        &mut self,
        schema: TableSchema,
    ) -> std::result::Result<(), String> {

        if self.tables.contains_key(&schema.name) {
            return Err("table already exists".into());
        }

        self.tables.insert(schema.name.clone(), schema);

        Ok(())
    }

    pub fn create_index(
        &mut self,
        index: IndexSchema,
    ) -> Result<()> {

        if self.indexes.contains_key(&index.name) {
            return Err(DatabaseError::Other(
                format!(
                    "index '{}' already exists",
                    index.name,
                ),
            ));
        }

        self.indexes.insert(
            index.name.clone(),
            index,
        );

        Ok(())
    }

    pub fn table(
        &self,
        name: &str,
    ) -> Option<&TableSchema> {
        self.tables.get(name)
    }

    pub fn index(
        &self,
        name: &str,
    ) -> Option<&IndexSchema> {

        self.indexes.get(name)
    }

    pub fn indexes(&self) -> impl Iterator<Item = &IndexSchema> {
        self.indexes.values()
    }

    pub fn exists(
        &self,
        name: &str,
    ) -> bool {
        self.tables.contains_key(name)
    }

    pub fn load_from_engine(
        &mut self,
        engine: &mut Engine,
    ) -> Result<()> {

        let mut iter = engine.iter()?;

        while let Some(record) = iter.next()? {

            if !record.key.starts_with("__schema__:") {
                continue;
            }

            let serialized = match record.value {

                Value::Data(data) => data,

                Value::Tombstone => continue,
            };

            let schema =
                TableSchema::deserialize(&serialized);

            self.create_table(schema)?;
        }

        Ok(())
    }

    pub fn indexes_for_table(
        &self,
        table_name: &str,
    ) -> Vec<&IndexSchema> {
        self.indexes
            .values()
            .filter(|index| index.table_name == table_name)
            .collect()
    }

    pub fn load_indexes_from_engine(
        &mut self,
        engine: &mut Engine,
    ) -> Result<()> {

        let mut iter = engine.iter()?;

        while let Some(record) = iter.next()? {

            if !record.key.starts_with("__index_meta__:") {
                continue;
            }

            let serialized = match record.value {

                Value::Data(data) => data,

                Value::Tombstone => continue,
            };

            let schema = IndexSchema::deserialize(
                &serialized,
            )?;

            // Ignore duplicates during recovery.
            if self.index(&schema.name).is_none() {

                self.create_index(schema)
                    .map_err(|e| DatabaseError::Other(format!(
                        "{:?}",
                        e,
                    )))?;
            }
        }

        Ok(())
    }
}

impl TableSchema {

    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    pub fn primary_key(&self) -> Option<&Column> {
        self.columns.iter().find(|c| c.primary_key)
    }

    pub fn serialize(&self) -> String {

        let mut result = self.name.clone();

        result.push('|');

        let columns = self.columns
            .iter()
            .map(|column| {

                let ty = match column.data_type {
                    CatalogDataType::Integer => "INTEGER",
                    CatalogDataType::Text => "TEXT",
                };

                format!("{}:{}", column.name, ty)

            })
            .collect::<Vec<_>>()
            .join(",");

        result.push_str(&columns);

        result
    }

    pub fn deserialize(data: &str) -> Self {

        let (table_name, columns_part) = data
            .split_once('|')
            .expect("Invalid schema format");

        let mut columns = Vec::new();

        if !columns_part.is_empty() {

            for column in columns_part.split(',') {

                let (name, ty) = column
                    .split_once(':')
                    .expect("Invalid column definition");

                let data_type = match ty {

                    "INTEGER" => CatalogDataType::Integer,

                    "TEXT" => CatalogDataType::Text,

                    _ => panic!("Unknown data type"),
                };

                columns.push(Column {
                    name: name.to_string(),
                    data_type,
                    nullable: false,
                    primary_key: false
                });
            }
        }

        Self {
            name: table_name.to_string(),
            columns,
        }
    }
}

impl IndexSchema {
    pub fn serialize(&self) -> String {
        format!(
            "{}|{}|{}",
            self.name,
            self.table_name,
            self.column_name,
        )
    }

    pub fn deserialize(s: &str) -> Result<Self> {
        let mut parts = s.split('|');

        let name = parts
            .next()
            .ok_or_else(|| DatabaseError::Other("invalid index schema".into()))?
            .to_string();

        let table_name = parts
            .next()
            .ok_or_else(|| DatabaseError::Other("invalid index schema".into()))?
            .to_string();

        let column_name = parts
            .next()
            .ok_or_else(|| DatabaseError::Other("invalid index schema".into()))?
            .to_string();

        Ok(Self {
            name,
            table_name,
            column_name,
        })
    }
}