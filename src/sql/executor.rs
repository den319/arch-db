use crate::{engine::{Engine, Value}, error::{DatabaseError, Result}, sql::{ast::{BinaryOperator, CreateTable, Delete, Expr, Insert, Select, SelectItem, Statement, Update}, catalog::{Catalog, Column, TableSchema}, expression::ExpressionEvaluator, row::{Row, RowValue}, table::Table}, storage_iterator::StorageIterator};

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

    fn can_use_primary_key_lookup(
        &self,
        table: &Table,
        expr: &Expr,
    ) -> bool {

        let pk = match table.schema.primary_key() {
            Some(pk) => pk,
            None => return false,
        };

        match expr {
            Expr::Binary { left, op, right } => {

                if *op != BinaryOperator::Equal {
                    return false;
                }

                match (&**left, &**right) {

                    (
                        Expr::Identifier(column),
                        Expr::Number(_),
                    ) if column == &pk.name => true,

                    (
                        Expr::Identifier(column),
                        Expr::String(_),
                    ) if column == &pk.name => true,

                    _ => false,
                }
            }

            _ => false,
        }
    }

    fn scan_table(
        &mut self,
        table_name: &str,
    ) -> Result<Vec<(String,Row)>> {

        let prefix = format!("{}:", table_name);

        let mut rows = Vec::new();

        let mut iter = self.engine.iter()?;

        while let Some(record) = iter.next()? {

            if !record.key.starts_with(&prefix) {
                continue;
            }

            match record.value {
                Value::Data(serialized) => {
                    let row= Row::deserialize(&serialized);
                    rows.push((record.key, row));
                }

                Value::Tombstone => {
                    continue;
                }
            }
        }

        Ok(rows)
    }

    fn matching_rows(
        &mut self,
        table_name: &str,
        where_clause: &Expr,
    ) -> Result<Vec<(String,Row)>> {

        let rows = self.scan_table(table_name)?;

        let mut result = Vec::new();

        for (key, row) in rows {

            if ExpressionEvaluator::evaluate(
                &row,
                where_clause,
            )? {

                result.push((key, row));
            }
        }

        Ok(result)
    }

    fn project_row(
        &self,
        row: &Row,
        columns: &[SelectItem],
        schema_columns: &[Column],
    ) -> Vec<String> {

        let mut values = Vec::new();

        for column in columns {

            match column {

                SelectItem::Wildcard => {

                    for col in schema_columns {

                        match row.get(&col.name) {

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

                SelectItem::Column(name) => {

                    match row.get(name) {

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

        values
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

        //----------------------------------------------------------
        // FAST PATH (PRIMARY KEY LOOKUP)
        //----------------------------------------------------------

        if let Some(expr) = &stmt.where_clause {

            if self.can_use_primary_key_lookup(&table, expr) {

                let key = match table.storage_key_from_expr(expr) {
                    Ok(key) => key,
                    Err(err) => {
                        return QueryResult::Message(format!(
                            "Error: {}",
                            err
                        ));
                    }
                };

                let value = match self.engine.get(&key) {

                    Some(Value::Data(data)) => data,

                    Some(Value::Tombstone) | None => {
                        return QueryResult::Rows(vec![]);
                    }
                };

                let row = Row::deserialize(&value);

                let values = self.project_row(
                    &row,
                    &stmt.columns,
                    &schema.columns,
                );

                return QueryResult::Rows(vec![values]);
            }
        }

        //----------------------------------------------------------
        // FALLBACK : TABLE SCAN
        //----------------------------------------------------------

        let rows = match &stmt.where_clause {

            Some(expr) => {

                match self.matching_rows(
                    &stmt.table_name,
                    expr,
                ) {
                    Ok(rows) => rows,
                    Err(err) => {
                        return QueryResult::Message(format!(
                            "Error: {}",
                            err
                        ));
                    }
                }
            }

            None => {

                match self.scan_table(&stmt.table_name) {

                    Ok(rows) => rows,

                    Err(err) => {
                        return QueryResult::Message(format!(
                            "Error: {}",
                            err
                        ));
                    }
                }
            }
        };

        let mut result = Vec::new();

        for (_, row) in rows {

            result.push(
                self.project_row(
                    &row,
                    &stmt.columns,
                    &schema.columns,
                )
            );
        }

        QueryResult::Rows(result)
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

        //----------------------------------------------------------
        // FAST PATH (PRIMARY KEY LOOKUP)
        //----------------------------------------------------------

        if self.can_use_primary_key_lookup(
            &table,
            &where_clause,
        ) {

            let key = match table.storage_key_from_expr(
                &where_clause,
            ) {
                Ok(key) => key,

                Err(err) => {
                    return QueryResult::Message(format!(
                        "Error: {}",
                        err
                    ));
                }
            };

            match self.engine.delete(key) {

                Ok(_) => {
                    return QueryResult::Message(
                        "1 row deleted".into(),
                    );
                }

                Err(err) => {
                    return QueryResult::Message(format!(
                        "Error: {}",
                        err
                    ));
                }
            }
        }

        //----------------------------------------------------------
        // FALLBACK : TABLE SCAN
        //----------------------------------------------------------

        let rows = match self.matching_rows(
            &stmt.table_name,
            &where_clause,
        ) {

            Ok(rows) => rows,

            Err(err) => {
                return QueryResult::Message(format!(
                    "Error: {}",
                    err
                ));
            }
        };

        if rows.is_empty() {
            return QueryResult::Message(
                "0 rows deleted".into(),
            );
        }

        let deleted = rows.len();

        for (key, _) in rows {

            if let Err(err) = self.engine.delete(key) {

                return QueryResult::Message(format!(
                    "Error: {}",
                    err
                ));
            }
        }

        QueryResult::Message(format!(
            "{} row(s) deleted",
            deleted,
        ))
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

        //----------------------------------------------------------
        // FAST PATH (PRIMARY KEY LOOKUP)
        //----------------------------------------------------------

        if self.can_use_primary_key_lookup(&table, &where_clause) {

            let key = match table.storage_key_from_expr(&where_clause) {
                Ok(key) => key,
                Err(err) => {
                    return QueryResult::Message(format!(
                        "Error: {}",
                        err
                    ));
                }
            };

            let row_data = match self.engine.get(&key) {

                Some(Value::Data(data)) => data,

                _ => {
                    return QueryResult::Message(
                        "Error: row not found".into(),
                    );
                }
            };

            let mut row = Row::deserialize(&row_data);

            let pk_name = schema
                .primary_key()
                .expect("table should have a primary key")
                .name
                .clone();

            for assignment in &stmt.assignments {

                if assignment.column == pk_name {

                    return QueryResult::Message(
                        "Error: updating the primary key is not supported".into(),
                    );
                }

                let value = match &assignment.value {

                    Expr::Number(n) => RowValue::Integer(*n),

                    Expr::String(s) => RowValue::Text(s.clone()),

                    _ => {
                        return QueryResult::Message(
                            "Error: unsupported expression".into(),
                        );
                    }
                };

                if let Err(_) = row.update(
                    &assignment.column,
                    value,
                ) {
                    return QueryResult::Message(format!(
                        "Error: unknown column '{}'",
                        assignment.column
                    ));
                }
            }

            let serialized = row.serialize();

            if let Err(err) = self.engine.put(
                key,
                serialized,
            ) {
                return QueryResult::Message(format!(
                    "Error: {}",
                    err
                ));
            }

            return QueryResult::Message(
                "1 row updated".into(),
            );
        }

        //----------------------------------------------------------
        // FALLBACK : TABLE SCAN
        //----------------------------------------------------------

        let rows = match self.matching_rows(
            &stmt.table_name,
            &where_clause,
        ) {
            Ok(rows) => rows,

            Err(err) => {
                return QueryResult::Message(format!(
                    "Error: {}",
                    err
                ));
            }
        };

        if rows.is_empty() {

            return QueryResult::Message(
                "0 rows updated".into(),
            );
        }

        let updated = rows.len();

        let pk_name = schema
            .primary_key()
            .expect("table should have a primary key")
            .name
            .clone();

        for (key, mut row) in rows {

            for assignment in &stmt.assignments {

                if assignment.column == pk_name {

                    return QueryResult::Message(
                        "Error: updating the primary key is not supported".into(),
                    );
                }

                let value = match &assignment.value {

                    Expr::Number(n) => RowValue::Integer(*n),

                    Expr::String(s) => RowValue::Text(s.clone()),

                    _ => {
                        return QueryResult::Message(
                            "Error: unsupported expression".into(),
                        );
                    }
                };

                if let Err(_) = row.update(
                    &assignment.column,
                    value,
                ) {
                    return QueryResult::Message(format!(
                        "Error: unknown column '{}'",
                        assignment.column
                    ));
                }
            }

            let serialized = row.serialize();

            if let Err(err) = self.engine.put(
                key,
                serialized,
            ) {
                return QueryResult::Message(format!(
                    "Error: {}",
                    err
                ));
            }
        }

        QueryResult::Message(format!(
            "{} row(s) updated",
            updated,
        ))
    }

}