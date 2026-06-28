use crate::{error::{DatabaseError, Result}, sql::{ast::{CreateTable, Delete, Expr, Insert, Select, Statement, Update}, catalog::{Catalog, Column, DataType as CatalogDataType, TableSchema}, row::{Row, Value}, table::Table}, storage::Storage};

#[derive(Debug, PartialEq)]
pub enum QueryResult {
    None,

    Message(String),

    Rows(Vec<Vec<String>>),
}

pub enum ExecutionResult {
    Success,
    Rows(Vec<Row>),
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
        statement: Statement,
    ) -> QueryResult {

        match statement {

            Statement::CreateTable(statement) =>
                self.execute_create_table(statement).unwrap_or_else(|e| {
                    QueryResult::Message(format!("Error: {}", e))
                }),

            Statement::Insert(statement) =>
                self.execute_insert(statement),

            Statement::Select(statement) =>
                self.execute_select(statement),

            Statement::Delete(statement) =>
                self.execute_delete(statement),

            Statement::Update(statement) =>
                self.execute_update(statement),
        }
    }

    pub fn execute_create_table(
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

    pub fn execute_insert(
        &mut self,
        stmt: Insert,
    ) -> Result<QueryResult> {
        let schema = self
            .catalog
            .table(&stmt.table_name)
            .ok_or(DatabaseError::Other(
                format!("table '{}' does not exist", stmt.table_name),
            ))?
            .clone();

        let table = Table::new(schema);

        let mut values = Vec::new();

        for expr in stmt.values {
            let value = match expr {
                Expr::Number(n) => Value::Integer(n),

                Expr::String(s) => Value::Text(s),

                _ => {
                    return Err(DatabaseError::Other(
                        "unsupported expression".into(),
                    ));
                }
            };

            values.push(value);
        }

        let row = Row::from_columns(
            stmt.columns,
            values,
        )
        .map_err(|e| DatabaseError::Other(e))?;

        println!("{:#?}", row);

        Ok(QueryResult::Message(
            "Insert parsed successfully".into(),
        ))
    }

    pub fn execute_select(
        &mut self,
        stmt: Select,
    ) -> QueryResult {
        todo!()
    }

    pub fn execute_delete(
        &mut self,
        stmt: Delete,
    ) -> QueryResult {
        todo!()
    }

    pub fn execute_update(
        &mut self,
        stmt: Update,
    ) -> QueryResult {
        todo!()
    }
}