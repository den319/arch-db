use crate::{engine::{Engine, Value}, error::{DatabaseError, Result}, sql::{ast::{CreateTable, Delete, Expr, Insert, Select, SelectItem, Statement, Update}, catalog::{Catalog, Column, TableSchema}, expression::ExpressionEvaluator, row::{Row, RowValue}, table::Table}, storage_iterator::StorageIterator};

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
    pub engine: &'a mut Engine,
}


impl<'a> Executor<'a> {
    pub fn new(
        catalog: &'a mut Catalog,
        engine: &'a mut Engine,
    ) -> Self {
        Self {
            catalog,
            engine,
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
                self.execute_insert(statement).unwrap_or_else(|e| {
                    QueryResult::Message(format!("Error: {}", e))
                }),

            Statement::Select(statement) =>
                self.execute_select(statement),

            Statement::Delete(statement) =>
                self.execute_delete(statement),

            Statement::Update(statement) =>
                self.execute_update(statement),
        }
    }

    fn expr_to_value(
        &self,
        expr: Expr,
    ) -> Result<RowValue> {
        match expr {
            Expr::Number(n) => Ok(RowValue::Integer(n)),

            Expr::String(s) => Ok(RowValue::Text(s)),

            _ => Err(DatabaseError::Other(
                "unsupported expression".into(),
            )),
        }
    }

    fn rows_affected_message(rows: usize) -> QueryResult {
        QueryResult::Message(format!(
            "Query OK, {} row{} affected",
            rows,
            if rows == 1 { "" } else { "s" }
        ))
    }

    fn scan_table(
        &mut self,
        table_name: &str,
    ) -> Result<Vec<Row>> {

        let prefix = format!("{}:", table_name);

        let mut rows = Vec::new();

        let mut iter = self.engine.iter()?;

        while let Some(record) = iter.next()? {

            if !record.key.starts_with(&prefix) {
                continue;
            }

            match record.value {
                Value::Data(serialized) => {
                    rows.push(Row::deserialize(&serialized));
                }

                Value::Tombstone => {
                    continue;
                }
            }
        }

        Ok(rows)
    }

    pub fn execute_create_table(
        &mut self,
        stmt: CreateTable,
    ) -> Result<QueryResult> {

        let columns = stmt
            .columns
            .into_iter()
            .enumerate()
            .map(|(i, column)| Column {
                name: column.name,
                data_type: column.data_type.into(),
                primary_key: i == 0,
                nullable: !(i == 0),
            })
            .collect();

        let schema = TableSchema {
            name: stmt.table_name,
            columns,
        };

        self.catalog.create_table(schema)
            .map_err(|e| DatabaseError::Other(format!("{:?}", e)))?;

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
            values.push(self.expr_to_value(expr)?);
        }

        let row = Row::from_columns(
            stmt.columns,
            values,
        )
        .map_err(|e| DatabaseError::Other(format!("{:?}", e)))?;

        let key = table
            .storage_key(&row)
            .ok_or(DatabaseError::Other(
                "missing primary key".into(),
            ))?;

        println!("row: {:#?}", row);
        println!("storage key = {}", key);

        let bytes = row.serialize();

        self.engine
            .put(key, bytes)?;

        Ok(QueryResult::Message(
            "Insert parsed successfully".into(),
        ))
    }

    pub fn execute_select(
        &mut self,
        stmt: Select,
    ) -> QueryResult {

        let result = (|| -> Result<QueryResult> {

            // Lookup schema
            let schema = self
                .catalog
                .table(&stmt.table_name)
                .ok_or(DatabaseError::Other(
                    format!(
                        "table '{}' does not exist",
                        stmt.table_name
                    ),
                ))?
                .clone();

            let table = Table::new(schema);

            if stmt.where_clause.is_none() {

                let rows = match self.scan_table(&stmt.table_name) {
                    Ok(rows) => rows,
                    Err(e) => {
                        return Ok(QueryResult::Message(format!("Error: {}", e)));
                    }
                };

                let mut result: Vec<Vec<String>> = Vec::new();

                for row in rows {

                    let mut values = Vec::new();

                    for column in &stmt.columns {
                        match column {
                            SelectItem::Wildcard => {
                                for (_, val) in &row.values {
                                    match val {
                                        RowValue::Integer(i) => values.push(i.to_string()),
                                        RowValue::Text(s) => values.push(s.clone()),
                                    }
                                }
                            }
                            SelectItem::Column(col_name) => {
                                match row.get(col_name) {
                                    Some(RowValue::Integer(i)) => {
                                        values.push(i.to_string());
                                    }
                                    Some(RowValue::Text(s)) => {
                                        values.push(s.clone());
                                    }
                                    None => {
                                        values.push("NULL".into());
                                    }
                                }
                            }
                        }
                    }

                        if let Some(expr) = &stmt.where_clause {
                            if !ExpressionEvaluator::evaluate(
                                &row,
                                expr,
                            )? {
                                continue;
                            }
                        }

                    result.push(values);
                }

                    return Ok(QueryResult::Rows(result));
            }

            // For now we only support:
            // WHERE <primary_key> = <literal>
            let where_clause = stmt
                .where_clause
                .ok_or(DatabaseError::Other(
                    "SELECT without WHERE is not supported yet".into(),
                ))?;

            let storage_key = match where_clause {

                Expr::Binary {
                    left,
                    op: _,
                    right,
                } => {

                    match (*left, *right) {

                        (Expr::Identifier(_), value_expr) => {

                            let value = self.expr_to_value(value_expr)?;

                            table
                                .storage_key_from_primary_key(&value)
                                .ok_or(DatabaseError::Other(
                                    "missing primary key".into(),
                                ))?
                        }

                        _ => {
                            return Err(DatabaseError::Other(
                                "unsupported WHERE clause".into(),
                            ));
                        }
                    }
                }

                _ => {
                    return Err(DatabaseError::Other(
                        "unsupported WHERE clause".into(),
                    ));
                }
            };

            let value = self
                .engine
                .get(&storage_key)
                .ok_or(DatabaseError::Other(
                    "row not found".into(),
                ))?;

            match value {

                crate::engine::Value::Data(serialized) => {

                    let row = Row::deserialize(&serialized);

                    let result = row
                        .values
                        .into_values()
                        .map(|value| match value {
                            RowValue::Integer(v) => v.to_string(),
                            RowValue::Text(v) => v,
                        })
                        .collect::<Vec<_>>();

                    Ok(QueryResult::Rows(vec![result]))
                }

                crate::engine::Value::Tombstone => {
                    Err(DatabaseError::Other(
                        "row not found".into(),
                    ))
                }
            }

        })();

        result.unwrap_or_else(|e| {
            QueryResult::Message(format!("Error: {}", e))
        })
    }

    pub fn execute_delete(
        &mut self,
        stmt: Delete,
    ) -> QueryResult {
        let schema = match self.catalog.table(&stmt.table_name) {
            Some(schema) => schema.clone(),
            None => {
                return QueryResult::Message(format!(
                    "Error: table '{}' does not exist",
                    stmt.table_name
                ));
            }
        };

        let table = Table::new(schema);

        let where_clause = match stmt.where_clause {
            Some(expr) => expr,
            None => {
                return QueryResult::Message(
                    "Error: DELETE without WHERE is not supported".into(),
                );
            }
        };

        let key = match table.storage_key_from_expr(&where_clause) {
            Ok(key) => key,
            Err(err) => {
                return QueryResult::Message(format!("Error: {}", err));
            }
        };

        match self.engine.delete(key) {
            Ok(_) => QueryResult::Message(
                "Row deleted successfully".into(),
            ),
            Err(err) => QueryResult::Message(format!(
                "Error: {}",
                err
            )),
        }
    }

    pub fn execute_update(
        &mut self,
        stmt: Update,
    ) -> QueryResult {
        // Check table exists
        let schema = match self.catalog.table(&stmt.table_name) {
            Some(schema) => schema.clone(),
            None => {
                return QueryResult::Message(format!(
                    "Error: table '{}' does not exist",
                    stmt.table_name
                ));
            }
        };

        let table = Table::new(schema.clone());

        // Require WHERE clause
        let where_clause = match stmt.where_clause {
            Some(expr) => expr,
            None => {
                return QueryResult::Message(
                    "Error: UPDATE without WHERE is not supported".into(),
                );
            }
        };

        // Build storage key from WHERE
        let key = match table.storage_key_from_expr(&where_clause) {
            Ok(key) => key,
            Err(err) => {
                return QueryResult::Message(format!("Error: {}", err));
            }
        };

        // Read existing row
        let row_data = match self.engine.get(&key) {
            Some(crate::engine::Value::Data(data)) => data,
            _ => {
                return QueryResult::Message(
                    "Error: row not found".into(),
                );
            }
        };

        // Deserialize
        let mut row = Row::deserialize(&row_data);

        // Primary key name
        let pk_name = schema
            .primary_key()
            .expect("table should have a primary key")
            .name
            .clone();

        // Apply assignments
        for assignment in stmt.assignments {
            // Don't allow PK updates
            if assignment.column == pk_name {
                return QueryResult::Message(
                    "Error: updating the primary key is not supported".into(),
                );
            }

            let value = match assignment.value {
                Expr::Number(n) => RowValue::Integer(n),
                Expr::String(s) => RowValue::Text(s),
                _ => {
                    return QueryResult::Message(
                        "Error: unsupported expression".into(),
                    );
                }
            };

            if let Err(_) = row.update(&assignment.column, value) {
                return QueryResult::Message(format!(
                    "Error: unknown column '{}'",
                    assignment.column
                ));
            }
        }

        // Write updated row
        let serialized = row.serialize();

        if let Err(err) = self.engine.put(key, serialized) {
            return QueryResult::Message(format!("Error: {}", err));
        }

        QueryResult::Message(
            "Row updated successfully".into(),
        )
    }
}