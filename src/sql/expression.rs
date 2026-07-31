use std::todo;

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
                Self::evaluate_equal(row, left, right)
            }

            BinaryOperator::NotEqual => {
                Self::evaluate_not_equal(row, left, right)
            }

            BinaryOperator::GreaterThan => {
                Self::evaluate_greater_than(row, left, right)
            }

            BinaryOperator::GreaterThanOrEqual => {
                Self::evaluate_greater_than_or_equal(row, left, right)
            }

            BinaryOperator::LessThan => {
                Self::evaluate_less_than(row, left, right)
            }

            BinaryOperator::LessThanOrEqual => {
                Self::evaluate_less_than_or_equal(row, left, right)
            }

            BinaryOperator::And => {
                Self::evaluate_and(row, left, right)
            }

            BinaryOperator::Or => {
                Self::evaluate_or(row, left, right)
            }
        }
    }

    fn evaluate_and(row: &Row, left: &Expr, right: &Expr) -> Result<bool> {
        let left_result =
            Self::evaluate(
                row,
                left,
            )?;

        if !left_result {
            return Ok(false);
        }

        let right_result =
            Self::evaluate(
                row,
                right,
            )?;

        Ok(right_result)
    }

        fn evaluate_or(row: &Row, left: &Expr, right: &Expr) -> Result<bool> {
        let left_result =
            Self::evaluate(
                row,
                left,
            )?;

        if left_result {
            return Ok(true);
        }

        let right_result =
            Self::evaluate(
                row,
                right,
            )?;

        Ok(right_result)
    }

    fn evaluate_equal(
        row: &Row,
        left: &Expr,
        right: &Expr,
    ) -> Result<bool> {

        let (lhs, rhs) = Self::values(row, left, right)?;

        match (lhs, rhs) {

            (
                RowValue::Integer(a),
                Expr::Number(b),
            ) => Ok(a == b),

            (
                RowValue::Text(a),
                Expr::String(b),
            ) => Ok(a == b),

            _ => Err("type mismatch in WHERE clause".into()),
        }
    }

    fn evaluate_not_equal(
        row: &Row,
        left: &Expr,
        right: &Expr,
    ) -> Result<bool> {

        Ok(!Self::evaluate_equal(row, left, right)?)
    }

    fn evaluate_greater_than(
        row: &Row,
        left: &Expr,
        right: &Expr,
    ) -> Result<bool> {

        let (lhs, rhs) = Self::values(row, left, right)?;

        match (lhs, rhs) {

            (
                RowValue::Integer(a),
                Expr::Number(b),
            ) => Ok(a > b),

            (
                RowValue::Text(a),
                Expr::String(b),
            ) => Ok(a > b),

            _ => Err("type mismatch in WHERE clause".into()),
        }
    }

    fn evaluate_greater_than_or_equal(
        row: &Row,
        left: &Expr,
        right: &Expr,
    ) -> Result<bool> {

        let (lhs, rhs) = Self::values(row, left, right)?;

        match (lhs, rhs) {

            (
                RowValue::Integer(a),
                Expr::Number(b),
            ) => Ok(a >= b),

            (
                RowValue::Text(a),
                Expr::String(b),
            ) => Ok(a >= b),

            _ => Err("type mismatch in WHERE clause".into()),
        }
    }

    fn evaluate_less_than(
        row: &Row,
        left: &Expr,
        right: &Expr,
    ) -> Result<bool> {

        let (lhs, rhs) = Self::values(row, left, right)?;

        match (lhs, rhs) {

            (
                RowValue::Integer(a),
                Expr::Number(b),
            ) => Ok(a < b),

            (
                RowValue::Text(a),
                Expr::String(b),
            ) => Ok(a < b),

            _ => Err("type mismatch in WHERE clause".into()),
        }
    }

    fn evaluate_less_than_or_equal(
        row: &Row,
        left: &Expr,
        right: &Expr,
    ) -> Result<bool> {

        let (lhs, rhs) = Self::values(row, left, right)?;

        match (lhs, rhs) {

            (
                RowValue::Integer(a),
                Expr::Number(b),
            ) => Ok(a <= b),

            (
                RowValue::Text(a),
                Expr::String(b),
            ) => Ok(a <= b),

            _ => Err("type mismatch in WHERE clause".into()),
        }
    }

    fn values(
        row: &Row,
        left: &Expr,
        right: &Expr,
    ) -> Result<(RowValue, Expr)> {

        let column = match left {
            Expr::Identifier(name) => name,
            _ => {
                return Err(
                    "left side of WHERE must be a column".into()
                );
            }
        };

        let value = match row.get(column) {
            Some(v) => v.clone(),
            None => {
                return Err(
                    format!("unknown column '{}'", column).into()
                );
            }
        };

        Ok((value, right.clone()))
    }
    
}