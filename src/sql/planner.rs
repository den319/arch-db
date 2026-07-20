use crate::sql::{ast::BinaryOperator, catalog::IndexSchema, row::RowValue};

#[derive(Debug, Clone)]
pub struct IndexLookup {
    // index: IndexSchema,
    pub column: String,
    pub operator: BinaryOperator,
    pub value: RowValue,
}