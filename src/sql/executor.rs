use crate::{error::{DatabaseError, Result}, sql::{ast::{CreateTable, Delete, Insert, Select, Statement, Update}, catalog::{Catalog, Column, DataType as CatalogDataType, TableSchema}}, storage::Storage};

#[derive(Debug, PartialEq)]
pub enum QueryResult {
    None,

    Message(String),

    Rows(Vec<Vec<String>>),
}

pub struct Executor<'a> {
    pub catalog: &'a mut Catalog,
    pub storage: &'a mut Storage,
}

impl<'a> Executor<'a> {
    pub fn new(
        catalog: &'a mut Catalog,
        storage: &'a mut Storage,
    ) -> Self {
        Self {
            catalog,
            storage,
        }
    }

    pub fn execute(
        &mut self,
        stmt: Statement,
    ) -> QueryResult {

        match stmt {

            Statement::CreateTable(stmt) =>
                self.execute_create_table(stmt).unwrap_or_else(|e| {
                    QueryResult::Message(format!("Error: {}", e))
                }),

            Statement::Insert(stmt) =>
                self.execute_insert(stmt),

            Statement::Select(stmt) =>
                self.execute_select(stmt),

            Statement::Delete(stmt) =>
                self.execute_delete(stmt),

            Statement::Update(stmt) =>
                self.execute_update(stmt),
        }
    }

    fn execute_create_table(
        &mut self,
        stmt: CreateTable,
    ) -> Result<QueryResult> {

        let columns = stmt
            .columns
            .into_iter()
            .map(|column| Column {
                name: column.name,
                data_type: column.data_type.into(),
                primary_key: false,
                nullable: true,
            })
            .collect();

        let schema = TableSchema {
            name: stmt.table_name,
            columns,
        };

        self.catalog.create_table(schema)
            .map_err(|e| DatabaseError::Other(e))?;

        Ok(QueryResult::Message(
            "Table created successfully".into(),
        ))
    }

    fn execute_insert(
        &mut self,
        stmt: Insert,
    ) -> QueryResult {
        todo!()
    }

    fn execute_select(
        &mut self,
        stmt: Select,
    ) -> QueryResult {
        todo!()
    }

    fn execute_delete(
        &mut self,
        stmt: Delete,
    ) -> QueryResult {
        todo!()
    }

    fn execute_update(
        &mut self,
        stmt: Update,
    ) -> QueryResult {
        todo!()
    }
}