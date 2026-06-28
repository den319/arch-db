use crate::sql::{
    catalog::TableSchema,
    row::Row,
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
            crate::sql::row::Value::Integer(v) => {
                Some(v.to_string())
            }

            crate::sql::row::Value::Text(v) => {
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
    
}