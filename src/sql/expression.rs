use crate::{
    error::Result, sql::{
        ast::{BinaryOperator, Expr}, row::{Row, RowValue},
    },
};

pub struct ExpressionEvaluator;

impl ExpressionEvaluator {
    pub fn evaluate(
        row: &Row,
        expr: &Expr,
    ) -> Result<bool> {

        match expr {

            Expr::Binary { left, op, right } => {
                Self::evaluate_binary(
                    row,
                    left,
                    op,
                    right,
                )
            }

            _ => Err("unsupported WHERE expression".into()),
        }
    }

    fn evaluate_binary(
        row: &Row,
        left: &Expr,
        op: &BinaryOperator,
        right: &Expr,
    ) -> Result<bool> {

        match op {

            BinaryOperator::Equal => {
                Self::evaluate_equal(
                    row,
                    left,
                    right,
                )
            }

        }
    }

    fn evaluate_equal(
        row: &Row,
        left: &Expr,
        right: &Expr,
    ) -> Result<bool> {
        let column = match left {

            Expr::Identifier(name) => name,

            _ => {
                return Err(
                    "left side of WHERE must be a column"
                        .into()
                );
            }
        };

        let row_value = match row.get(column) {

            Some(value) => value,

            None => {
                return Err(
                    format!("unknown column '{}'", column)
                        .into()
                );
            }
        };

        match (row_value, right) {

            (
                RowValue::Integer(lhs),
                Expr::Number(rhs),
            ) => {
                Ok(*lhs == *rhs)
            }

            (
                RowValue::Text(lhs),
                Expr::String(rhs),
            ) => {
                Ok(lhs == rhs)
            }

            _ => Err(
                "type mismatch in WHERE clause".into()
            ),
        }

        
    }
}