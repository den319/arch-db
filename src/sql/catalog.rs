use std::collections::HashMap;

use crate::{engine::{Engine, Value}, sql::ast::{self, DataType}, storage_iterator::StorageIterator};

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

pub struct Catalog {
    tables: HashMap<String, TableSchema>,
}

impl TableSchema {
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    pub fn primary_key(&self) -> Option<&Column> {
        self.columns.iter().find(|c| c.primary_key)
    }
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
        }
    }

    pub fn create_table(
        &mut self,
        schema: TableSchema,
    ) -> Result<(), String> {

        if self.tables.contains_key(&schema.name) {
            return Err("table already exists".into());
        }

        self.tables.insert(schema.name.clone(), schema);

        Ok(())
    }

    pub fn table(
        &self,
        name: &str,
    ) -> Option<&TableSchema> {
        self.tables.get(name)
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
    ) -> Result<()``> {

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
}

impl TableSchema {

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