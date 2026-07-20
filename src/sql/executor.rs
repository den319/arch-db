use crate::{engine::{Engine, Value}, error::{DatabaseError, Result}, sql::{ast::{BinaryOperator, CreateIndex, CreateTable, Delete, Expr, Insert, OrderDirection, Select, SelectItem, Statement, Update}, catalog::{Catalog, CatalogDataType, Column, IndexSchema, TableSchema}, expression::ExpressionEvaluator, planner::IndexLookup, row::{Row, RowValue}, table::Table}, storage_iterator::StorageIterator};

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

            Statement::CreateIndex(stmt) => {
                match self.execute_create_index(stmt) {
                    Ok(result) => result,
                    Err(err) => QueryResult::Message(format!(
                        "Error: {}",
                        err,
                    )),
                }
            }

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

    fn lookup_index(
        &mut self,
        table_name: &str,
        lookup: &IndexLookup,
    ) -> Result<Vec<String>> {

        let encoded =
            Table::encode_index_value(&lookup.value);

        let prefix = format!(
            "__index__:{}:{}:{}:",
            table_name,
            lookup.column,
            encoded,
        );

        let mut keys = Vec::new();

        let (start, end) =
            self.build_index_range(
                table_name,
                lookup,
            );

        let mut iter =
            self.engine.range_scan(
                &start,
                &end,
            )?;

        while let Some(record) = iter.next()? {

            //------------------------------------------------------
            // Index key format:
            //
            // __index__:users:name:Alice:1
            //------------------------------------------------------

            if let Some(primary_key) =
                record.key.rsplit(':').next()
            {
                keys.push(primary_key.to_string());
            }
        }

        Ok(keys)
    }

    fn fetch_rows_by_primary_keys(
        &mut self,
        table_name: &str,
        primary_keys: Vec<String>,
    ) -> Result<Vec<(String, Row)>> {

        let schema = self
            .catalog
            .table(table_name)
            .expect("table should exist")
            .clone();

        let table = Table::new(schema);

        let mut rows = Vec::new();

        for pk in primary_keys {

            // Determine the primary key type
            let pk_column = table
                .schema
                .primary_key()
                .expect("table should have a primary key");

            let pk_value = match pk_column.data_type {

                CatalogDataType::Integer => {
                    RowValue::Integer(
                        pk.parse().map_err(|_| {
                            DatabaseError::Other(
                                format!("Invalid integer primary key '{}'", pk)
                            )
                        })?
                    )
                }

                CatalogDataType::Text => {
                    RowValue::Text(pk)
                }
            };

            let storage_key = table
                .storage_key_from_primary_key(&pk_value)
                .expect("table has a primary key");

            if let Some(Value::Data(data)) =
                self.engine.get(&storage_key)
            {
                rows.push((
                    storage_key,
                    Row::deserialize(&data),
                ));
            }
        }

        Ok(rows)
    }

    fn fetch_matching_rows(
        &mut self,
        table_name: &str,
        where_clause: &Expr,
    ) -> Result<Vec<(String,Row)>> {

        let rows = if let Some(lookup) =
            self.find_usable_index(
                table_name,
                where_clause,
            )
        {
            let primary_keys =
                self.lookup_index(
                    table_name,
                    &lookup,
                )?;

            println!(
                "[Planner] Using index on {} ({})",
                table_name,
                &lookup.column,
            );

            self.fetch_rows_by_primary_keys(
                table_name,
                primary_keys,
            )?
        }
        else {
            println!(
                "[Planner] Table scan"
            );
            self.scan_table(table_name)?
        };
        
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

    fn build_index_range(
        &self,
        table_name: &str,
        lookup: &IndexLookup,
    ) -> (String, String) {

        let encoded =
            Table::encode_index_value(&lookup.value);

        let base = format!(
            "__index__:{}:{}:",
            table_name,
            lookup.column,
        );

        match lookup.operator {

            //------------------------------------------------------
            // =
            //------------------------------------------------------

            BinaryOperator::Equal => {

                let start = format!(
                    "{}{}:",
                    base,
                    encoded,
                );

                let mut end = start.clone();
                end.push(char::MAX);

                (start, end)
            }

            //------------------------------------------------------
            // >
            //------------------------------------------------------

            BinaryOperator::GreaterThan => {

                let mut start = format!(
                    "{}{}:",
                    base,
                    encoded,
                );

                start.push(char::MAX);

                let mut end = base.clone();
                end.push(char::MAX);

                (start, end)
            }

            //------------------------------------------------------
            // >=
            //------------------------------------------------------

            BinaryOperator::GreaterThanOrEqual => {

                let start = format!(
                    "{}{}:",
                    base,
                    encoded,
                );

                let mut end = base.clone();
                end.push(char::MAX);

                (start, end)
            }

            //------------------------------------------------------
            // <
            //------------------------------------------------------

            BinaryOperator::LessThan => {

                let start = base.clone();

                let end = format!(
                    "{}{}:",
                    base,
                    encoded,
                );

                (start, end)
            }

            //------------------------------------------------------
            // <=
            //------------------------------------------------------

            BinaryOperator::LessThanOrEqual => {

                let start = base.clone();

                let mut end = format!(
                    "{}{}:",
                    base,
                    encoded,
                );

                end.push(char::MAX);

                (start, end)
            }

            _ => unreachable!(),
        }
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

    fn build_index(
        &mut self,
        schema: &TableSchema,
        index: &IndexSchema,
    ) -> Result<()> {

        // Scan every row in the table.
        let rows = self.scan_table(&schema.name)?;

        // Locate the primary-key column.
        let pk_column = schema
            .columns
            .iter()
            .find(|c| c.primary_key)
            .ok_or_else(|| {
                DatabaseError::Other(
                    "table has no primary key".into(),
                )
            })?;

        for (_, row) in rows {


            let indexed_value = row
                .get(&index.column_name)
                .ok_or_else(|| DatabaseError::Other(format!(
                        "column: '{}' missing from row",
                        index.column_name,
                    )))?
                .as_storage_string();

            let primary_key = row
                .get(&pk_column.name)
                .ok_or_else(|| DatabaseError::Other(format!(
                        "primary key: '{}' missing from row",
                        index.column_name,
                    )))?
                .as_storage_string();

            //----------------------------------------------------------
            // Build storage key
            //----------------------------------------------------------

            let storage_key = Self::make_index_storage_key(
    &schema.name,
                &index.column_name,
                row.get(&index.column_name).unwrap(),
                row.get(&pk_column.name).unwrap(),
            );

            self.engine.put(
                storage_key,
                String::new(),
            )?;
        }

        Ok(())
    }

    fn insert_index_entries(
        &mut self,
        schema: &TableSchema,
        row: &Row,
    ) -> Result<()> {

        // Locate the primary-key column.
        let pk_column = schema
            .columns
            .iter()
            .find(|c| c.primary_key)
            .ok_or_else(|| {
                DatabaseError::Other(
                    "table has no primary key".into(),
                )
            })?;

        let pk_value = row
            .get(&pk_column.name)
            .ok_or_else(|| {
                DatabaseError::Other(
                    "primary key missing from row".into(),
                )
            })?;

        // Walk through every index.
        for index in self.catalog.indexes_for_table(&schema.name) {

            let column_value = row
                .get(&index.column_name)
                .ok_or_else(|| {
                    DatabaseError::Other(format!(
                        "column '{}' missing from row",
                        index.column_name,
                    ))
                })?;

            let storage_key = Self::make_index_storage_key(
                &schema.name,
                &index.column_name,
                column_value,
                pk_value,
            );

            self.engine.put(
                storage_key,
                String::new(),
            )?;
        }

        Ok(())
    }
    

    pub fn indexes_for_table(
        &self,
        table_name: &str,
    ) -> Vec<&IndexSchema> {

        self.catalog.indexes
            .values()
            .filter(|idx| idx.table_name == table_name)
            .collect()
    }

    fn delete_index_entries(
        &mut self,
        schema: &TableSchema,
        row: &Row,
    ) -> Result<()> {

        //----------------------------------------------------------
        // Locate primary-key column
        //----------------------------------------------------------

        let pk_column = schema
            .columns
            .iter()
            .find(|c| c.primary_key)
            .ok_or_else(|| {
                DatabaseError::Other(
                    "table has no primary key".into(),
                )
            })?;

        let pk_value = row
            .get(&pk_column.name)
            .ok_or_else(|| {
                DatabaseError::Other(
                    "primary key missing".into(),
                )
            })?;

        //----------------------------------------------------------
        // Remove every index entry
        //----------------------------------------------------------

        for index in self.catalog.indexes_for_table(&schema.name) {

            let column_value = row
                .get(&index.column_name)
                .ok_or_else(|| {
                    DatabaseError::Other(format!(
                        "column '{}' missing",
                        index.column_name,
                    ))
                })?;

            let storage_key = Self::make_index_storage_key(
                &schema.name,
                &index.column_name,
                column_value,
                pk_value,
            );

            self.engine.delete(storage_key)?;
        }

        Ok(())
    }

    fn find_usable_index(
        &self,
        table_name: &str,
        expr: &Expr,
    ) -> Option<IndexLookup> {

        //------------------------------------------------------
        // Only support:
        //
        // column <op> literal
        //------------------------------------------------------

        let (column, operator, value) = match expr {

            Expr::Binary {
                left,
                op,
                right,
            } => {

                match op {
                    BinaryOperator::Equal
                    | BinaryOperator::GreaterThan
                    | BinaryOperator::GreaterThanOrEqual
                    | BinaryOperator::LessThan
                    | BinaryOperator::LessThanOrEqual => {}

                    _ => return None,
                }

                let column = match &**left {
                    Expr::Identifier(name) => name.clone(),
                    _ => return None,
                };

                let value = match &**right {

                    Expr::Number(n) => {
                        RowValue::Integer(*n)
                    }

                    Expr::String(s) => {
                        RowValue::Text(s.clone())
                    }

                    _ => return None,
                };

                (column, op.clone(), value)
            }

            _ => return None,
        };

        //------------------------------------------------------
        // Does an index exist?
        //------------------------------------------------------

        let indexes =
            self.catalog.indexes_for_table(table_name);

        for index in indexes {

            if index.column_name == column {

                return Some(IndexLookup {
                    column,
                    operator,
                    value,
                });
            }
        }

        None
    }

    pub fn execute_create_table(
        &mut self,
        stmt: CreateTable,
    ) -> Result<QueryResult> {

        let primary_key_count = stmt
            .columns
            .iter()
            .filter(|column| column.primary_key)
            .count();

        match primary_key_count {
            0 => {
                return Err(DatabaseError::Other(
                    "table must contain exactly one PRIMARY KEY".into(),
                ));
            }

            1 => {}

            _ => {
                return Err(DatabaseError::Other(
                    "multiple PRIMARY KEY columns are not allowed".into(),
                ));
            }
        }

        let columns = stmt
            .columns
            .into_iter()
            .enumerate()
            .map(|(i, column)| Column {
                name: column.name,
                data_type: column.data_type.into(),
                primary_key: column.primary_key,
                nullable: !column.primary_key,
            })
            .collect();

        let schema = TableSchema {
            name: stmt.table_name.clone(),
            columns,
        };

        self.catalog.create_table(schema.clone())
            .map_err(|e| DatabaseError::Other(format!("{:?}", e)))?;

        let storage_key = format!(
            "__schema__:{}",
            stmt.table_name
        );
        let serialized = schema.serialize();

        if let Err(err) = self.engine.put(
            storage_key,
            serialized,
        ) {
            return Err(DatabaseError::Other(
                format!("Error: {}", err)
            ));
        }
        
        Ok(QueryResult::Message(
            "Table created successfully".into(),
        ))
    }

    fn make_index_storage_key(
        table_name: &str,
        column_name: &str,
        column_value: &RowValue,
        primary_key: &RowValue,
    ) -> String {

        let value = column_value;
        let pk = primary_key.as_storage_string();

        let encoded =
            Table::encode_index_value(&value);

        format!(
            "__index__:{}:{}:{}:{}",
            table_name,
            column_name,
            encoded,
            pk,
        )
    }

    pub fn execute_create_index(
        &mut self,
        stmt: CreateIndex,
    ) -> Result<QueryResult> {

        //----------------------------------------------------------
        // Validate table
        //----------------------------------------------------------

        let schema = match self.catalog.table(&stmt.table_name) {

            Some(schema) => schema.clone(),

            None => {
                return Err(DatabaseError::Other(format!(
                    "table '{}' does not exist",
                    stmt.table_name,
                )));
            }
        };

        //----------------------------------------------------------
        // Validate column
        //----------------------------------------------------------

        if !schema
            .columns
            .iter()
            .any(|c| c.name == stmt.column_name)
        {
            return Err(DatabaseError::Other(format!(
                "unknown column '{}'",
                stmt.column_name,
            )));
        }

        //----------------------------------------------------------
        // Register metadata
        //----------------------------------------------------------

        let index = IndexSchema {
            name: stmt.index_name.clone(),
            table_name: stmt.table_name.clone(),
            column_name: stmt.column_name.clone(),
        };

        self.catalog.create_index(index.clone())?;

        let index_schema = IndexSchema {
            name: stmt.index_name.clone(),
            table_name: stmt.table_name.clone(),
            column_name: stmt.column_name.clone(),
        };

        let storage_key = format!(
            "__index_meta__:{}",
            index_schema.name,
        );

        if let Err(err) = self.engine.put(
            storage_key,
            index_schema.serialize(),
        ) {
            return Ok(QueryResult::Message(format!(
                "Error: {}",
                err
            )))
        }

        //----------------------------------------------------------
        // Build physical index
        //----------------------------------------------------------

        self.build_index(
            &schema,
            &index,
        )?;

        Ok(QueryResult::Message(
            "Index created successfully".into(),
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

        let table = Table::new(schema.clone());

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

        self.insert_index_entries(
            &schema,
            &row,
        )?;

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

                match self.fetch_matching_rows(
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

        let mut filtered_rows = Vec::new();

        for (_, row) in rows {

            // result.push(
            //     self.project_row(
            //         &row,
            //         &stmt.columns,
            //         &schema.columns,
            //     )
            // );

            filtered_rows.push(row);
        }

        if let Some(order) = &stmt.order_by {

            filtered_rows.sort_by(|left, right| {

                let left_value = left.get(&order.column);

                let right_value = right.get(&order.column);

                match (left_value, right_value) {

                    (
                        Some(RowValue::Integer(a)),
                        Some(RowValue::Integer(b)),
                    ) => {

                        match order.direction {

                            OrderDirection::Asc => a.cmp(b),

                            OrderDirection::Desc => b.cmp(a),
                        }
                    }

                    (
                        Some(RowValue::Text(a)),
                        Some(RowValue::Text(b)),
                    ) => {

                        match order.direction {

                            OrderDirection::Asc => a.cmp(b),

                            OrderDirection::Desc => b.cmp(a),
                        }
                    }

                    _ => std::cmp::Ordering::Equal,
                }
            });
        }

        //----------------------------------------------------------
        // LIMIT
        //----------------------------------------------------------

        if let Some(limit) = stmt.limit {

            filtered_rows.truncate(limit);
        }

        let mut result = Vec::new();

        for row in filtered_rows {

            let values = self.project_row(
                &row,
                &stmt.columns,
                &schema.columns,
            );

            result.push(values);
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

            let rows = match self.fetch_matching_rows(
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

            let (key, row) = &rows[0];

            if let Err(err) = self.delete_index_entries(
                &table.schema,
                row,
            ) {
                return QueryResult::Message(format!(
                    "Error: {}",
                    err
                ));
            }

            return match self.engine.delete(key.clone()) {

                Ok(_) => QueryResult::Message(
                    "1 row deleted".into(),
                ),

                Err(err) => QueryResult::Message(format!(
                    "Error: {}",
                    err
                )),
            };
        }

        //----------------------------------------------------------
        // FALLBACK : TABLE SCAN
        //----------------------------------------------------------

        let rows = match self.fetch_matching_rows(
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

        for (key, row) in rows {

            if let Err(err) = self.delete_index_entries(
                &table.schema,
                &row,
            ) {
                return QueryResult::Message(format!(
                    "Error: {}",
                    err
                ));
            }

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

            let old_row = row.clone();

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

            // Remove old index entries.
            if let Err(err) = self.delete_index_entries(
                &schema,
                &old_row,
            ) {
                return QueryResult::Message(format!(
                    "Error: {}",
                    err
                ));
            }

            let serialized = row.serialize();

            if let Err(err) = self.engine.put(
                key.clone(),
                serialized,
            ) {
                return QueryResult::Message(format!(
                    "Error: {}",
                    err
                ));
            }

            // Insert new index entries.
            if let Err(err) = self.insert_index_entries(
                &schema,
                &row,
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

        let rows = match self.fetch_matching_rows(
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

            let old_row = row.clone();

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

            // Remove old index entries.
            if let Err(err) = self.delete_index_entries(
                &schema,
                &old_row,
            ) {
                return QueryResult::Message(format!(
                    "Error: {}",
                    err
                ));
            }

            let serialized = row.serialize();

            if let Err(err) = self.engine.put(
                key.clone(),
                serialized,
            ) {
                return QueryResult::Message(format!(
                    "Error: {}",
                    err
                ));
            }

            // Insert updated index entries.
            if let Err(err) = self.insert_index_entries(
                &schema,
                &row,
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