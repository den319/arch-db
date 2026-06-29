use crate::sql::{
    ast::{BinaryOperator, Expr}, catalog::TableSchema, row::{Row, RowValue},
};

#[derive(Debug)]
pub struct Table {
    pub schema: TableSchema,
}

impl Table {
    pub fn new(schema: TableSchema) -> Self {
        Self { schema }
    }

    pub fn primary_key_value(
        &self,
        row: &Row,
    ) -> Option<String> {
        let pk = self.schema.primary_key()?;

        let value = row.get(&pk.name)?;

        match value {
            RowValue::Integer(v) => {
                Some(v.to_string())
            }

            RowValue::Text(v) => {
                Some(v.clone())
            }
        }
    }

    pub fn storage_key(
        &self,
        row: &Row,
    ) -> Option<String> {
        let pk = self.primary_key_value(row)?;

        Some(format!(
            "{}:{}",
            self.schema.name,
            pk
        ))
    }

    pub fn storage_key_from_primary_key(
        &self,
        value: &RowValue,
    ) -> Option<String> {
        // Ensure the table actually has a primary key.
        self.schema.primary_key()?;

        let key = match value {
            RowValue::Integer(v) => v.to_string(),
            RowValue::Text(v) => v.clone(),
        };

        Some(format!("{}:{}", self.schema.name, key))
    }

    pub fn storage_key_from_expr(
        &self,
        expr: &Expr,
    ) -> Result<String, String> {
        let pk = self
            .schema
            .primary_key()
            .ok_or("table has no primary key")?;

        match expr {
            Expr::Binary {
                left,
                op: BinaryOperator::Equal,
                right,
            } => {
                let column = match left.as_ref() {
                    Expr::Identifier(name) => name,
                    _ => {
                        return Err(
                            "left side of WHERE must be a column".into(),
                        );
                    }
                };

                if column != &pk.name {
                    return Err(format!(
                        "WHERE must use primary key '{}'",
                        pk.name
                    ));
                }

                let value = match right.as_ref() {
                    Expr::Number(n) => n.to_string(),

                    Expr::String(s) => s.clone(),

                    _ => {
                        return Err(
                            "unsupported WHERE value".into(),
                        );
                    }
                };

                Ok(format!("{}:{}", self.schema.name, value))
            }

            _ => Err("unsupported WHERE clause".into()),
        }
    }
    
}