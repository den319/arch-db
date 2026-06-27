use std::collections::HashMap;

use crate::sql::ast;

#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Integer,
    Text,
}

#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub data_type: DataType,
    pub primary_key: bool,
    pub nullable: bool,
}

#[derive(Debug, Clone)]
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

impl From<ast::DataType> for DataType {
    fn from(value: ast::DataType) -> Self {
        match value {
            ast::DataType::Int => DataType::Integer,
            ast::DataType::Text => DataType::Text,
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
}