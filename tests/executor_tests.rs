use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use arch_db::sql::ast::{CreateIndex, OrderBy, OrderDirection};
use arch_db::sql::catalog::IndexSchema;
use arch_db::sql::lexer::Lexer;
use arch_db::sql::sql_parser::SQLParser;
use arch_db::{
    engine::{Engine, Value},
    sql::{
        ast::{
            self, Assignment, BinaryOperator, ColumnDef, CreateTable, DataType, Delete, Expr,
            Insert, Select, SelectItem, Statement, Update,
        },
        catalog::{self as catalog_mod, Catalog, CatalogDataType, Column, TableSchema},
        executor::{Executor, QueryResult},
        row::{Row, RowValue},
    },
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn make_engine() -> Engine {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = format!("storage/tests/test_executor_{}", id);
    let _ = fs::remove_dir_all(&path);
    Engine::with_storage_path(&path)
}

fn create_executor() -> Executor<'static> {
    let catalog = Catalog::new();
    let engine = make_engine();
    Executor::new(Box::leak(Box::new(catalog)), Box::leak(Box::new(engine)))
}

fn execute_sql(executor: &mut Executor, sql: &str) -> QueryResult {
    let lexer = Lexer::new(sql);
    let mut parser = SQLParser::new(lexer);
    let statement = parser.parse_statement();
    executor.execute(statement)
}

fn assert_rows_eq(result: QueryResult, expected: Vec<Vec<&str>>) {
    match result {
        QueryResult::Rows(rows) => {
            let expected: Vec<Vec<String>> = expected
                .into_iter()
                .map(|row| row.into_iter().map(|s| s.to_string()).collect())
                .collect();
            assert_eq!(rows, expected);
        }
        other => panic!("Expected Rows, got {:?}", other),
    }
}

fn extract_rows(result: &QueryResult) -> &Vec<Vec<String>> {
    match result {
        QueryResult::Rows(rows) => rows,
        other => panic!("Expected Rows, got {:?}", other),
    }
}

fn scan_index_keys(engine: &mut Engine) -> Vec<String> {
    engine
        .scan("__index__", "__index__~")
        .into_iter()
        .map(|(k, _)| k)
        .collect()
}

fn create_users_table(executor: &mut Executor) {
    executor.execute(Statement::CreateTable(CreateTable {
        table_name: "users".into(),
        columns: vec![
            ColumnDef {
                name: "id".into(),
                data_type: DataType::Int,
                primary_key: true,
            },
            ColumnDef {
                name: "name".into(),
                data_type: DataType::Text,
                primary_key: false,
            },
        ],
    }));
}

fn assert_rows_eq_sorted(result: QueryResult, expected: Vec<Vec<&str>>) {
    let mut actual = match result {
        QueryResult::Rows(rows) => rows,
        other => panic!("Expected Rows, got {:?}", other),
    };
    actual.sort();
    let mut expected: Vec<Vec<String>> = expected
        .into_iter()
        .map(|row| row.into_iter().map(|s| s.to_string()).collect())
        .collect();
    expected.sort();
    assert_eq!(actual, expected);
}

// =============================================================
// BASIC TESTS
// =============================================================

#[test]
fn test_create_table() {
    let mut executor = create_executor();

    let result = execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    assert_eq!(
        result,
        QueryResult::Message("Table created successfully".into())
    );
}

#[test]
fn test_insert_and_select() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );

    let result = execute_sql(&mut executor, "SELECT * FROM users;");

    assert_rows_eq(result, vec![vec!["1", "Alice"]]);
}

#[test]
fn test_insert_and_select_multiple_rows() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(&mut executor, "SELECT * FROM users;");

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice"], vec!["2", "Bob"]],
    );
}

#[test]
fn test_select_with_where() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE id = 1;",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice"]]);
}

#[test]
fn test_select_with_where_string() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE name = 'Alice';",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice"]]);
}

#[test]
fn test_select_columns() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT name FROM users;",
    );

    assert_rows_eq(result, vec![vec!["Alice"]]);
}

#[test]
fn test_select_where_not_found() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE id = 999;",
    );

    assert_rows_eq(result, vec![]);
}

#[test]
fn test_delete() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "DELETE FROM users WHERE id = 1;",
    );

    assert_eq!(result, QueryResult::Message("1 row deleted".into()));

    let result = execute_sql(&mut executor, "SELECT * FROM users;");

    assert_rows_eq(result, vec![vec!["2", "Bob"]]);
}

#[test]
fn test_delete_not_found() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );

    let result = execute_sql(
        &mut executor,
        "DELETE FROM users WHERE id = 999;",
    );

    assert_eq!(result, QueryResult::Message("0 rows deleted".into()));
}

#[test]
fn test_update() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );

    let result = execute_sql(
        &mut executor,
        "UPDATE users SET name = 'Bob' WHERE id = 1;",
    );

    assert_eq!(result, QueryResult::Message("1 row updated".into()));

    let result = execute_sql(&mut executor, "SELECT * FROM users;");

    assert_rows_eq(result, vec![vec!["1", "Bob"]]);
}

#[test]
fn test_update_not_found() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );

    let result = execute_sql(
        &mut executor,
        "UPDATE users SET name = 'Bob' WHERE id = 999;",
    );

    assert_eq!(result, QueryResult::Message("0 rows updated".into()));
}

#[test]
fn test_insert_without_column_list() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users VALUES (1, 'Alice');",
    );

    let result = execute_sql(&mut executor, "SELECT * FROM users;");

    assert_rows_eq(result, vec![vec!["1", "Alice"]]);
}

#[test]
fn test_select_order_by_asc() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users ORDER BY id ASC;",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice"], vec!["2", "Bob"]]);
}

#[test]
fn test_select_order_by_desc() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users ORDER BY id DESC;",
    );

    assert_rows_eq(result, vec![vec!["2", "Bob"], vec!["1", "Alice"]]);
}

#[test]
fn test_select_limit() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users LIMIT 1;",
    );

    assert_eq!(extract_rows(&result).len(), 1);
}

#[test]
fn test_select_order_by_limit() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (3, 'Charlie');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users ORDER BY id ASC LIMIT 2;",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice"], vec!["2", "Bob"]],
    );
}

#[test]
fn test_select_where_not_equal() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE id != 1;",
    );

    assert_rows_eq(result, vec![vec!["2", "Bob"]]);
}

#[test]
fn test_select_where_greater_than() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE id > 1;",
    );

    assert_rows_eq(result, vec![vec!["2", "Bob"]]);
}

#[test]
fn test_select_where_greater_than_or_equal() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE id >= 2;",
    );

    assert_rows_eq(result, vec![vec!["2", "Bob"]]);
}

#[test]
fn test_select_where_less_than() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE id < 2;",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice"]]);
}

#[test]
fn test_select_where_less_than_or_equal() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE id <= 1;",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice"]]);
}

#[test]
fn test_select_where_text_not_equal() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE name != 'Alice';",
    );

    assert_rows_eq(result, vec![vec!["2", "Bob"]]);
}

#[test]
fn test_select_where_text_greater_than() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE name > 'Alice';",
    );

    assert_rows_eq(result, vec![vec!["2", "Bob"]]);
}

#[test]
fn test_select_where_text_less_than() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE name < 'Bob';",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice"]]);
}

#[test]
fn test_select_where_text_greater_than_or_equal() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE name >= 'Bob';",
    );

    assert_rows_eq(result, vec![vec!["2", "Bob"]]);
}

#[test]
fn test_select_where_text_less_than_or_equal() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE name <= 'Alice';",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice"]]);
}

#[test]
fn test_select_where_and() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 30);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age = 30 AND name = 'Alice';",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice", "30"]]);
}

#[test]
fn test_select_where_or() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age = 30 OR name = 'Bob';",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["2", "Bob", "25"]],
    );
}

#[test]
fn test_select_where_and_or_precedence() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age > 25 AND name = 'Alice' OR name = 'Charlie';",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["3", "Charlie", "35"]],
    );
}

#[test]
fn test_select_where_parentheses() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age > 25 AND name = 'Alice') OR name = 'Charlie';",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["3", "Charlie", "35"]],
    );
}

#[test]
fn test_select_where_parentheses_different_precedence() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age > 25 AND (name = 'Alice' OR name = 'Charlie');",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["3", "Charlie", "35"]],
    );
}

#[test]
fn test_select_where_parentheses_override_precedence() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age > 25 OR name = 'Bob') AND name = 'Charlie';",
    );

    assert_rows_eq(result, vec![vec!["3", "Charlie", "35"]]);
}

#[test]
fn test_select_where_nested_parentheses() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age > 25 AND (name = 'Alice' OR name = 'Charlie')) OR name = 'David';",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["4", "David", "40"],
        ],
    );
}

#[test]
fn test_select_where_and_with_text() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, city TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, city) VALUES (1, 'Alice', 'New York');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, city) VALUES (2, 'Bob', 'Los Angeles');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE name = 'Alice' AND city = 'New York';",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice", "New York"]]);
}

#[test]
fn test_select_where_or_with_text() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, city TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, city) VALUES (1, 'Alice', 'New York');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, city) VALUES (2, 'Bob', 'Los Angeles');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE name = 'Alice' OR city = 'Los Angeles';",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "New York"], vec!["2", "Bob", "Los Angeles"]],
    );
}

#[test]
fn test_select_where_and_or_mixed() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT, city TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (1, 'Alice', 30, 'New York');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (2, 'Bob', 25, 'Los Angeles');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (3, 'Charlie', 35, 'Chicago');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age > 25 AND name = 'Alice' OR city = 'Chicago';",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30", "New York"],
            vec!["3", "Charlie", "35", "Chicago"],
        ],
    );
}

#[test]
fn test_select_where_and_or_mixed_with_parentheses() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT, city TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (1, 'Alice', 30, 'New York');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (2, 'Bob', 25, 'Los Angeles');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (3, 'Charlie', 35, 'Chicago');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age > 25 AND name = 'Alice') OR city = 'Chicago';",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30", "New York"],
            vec!["3", "Charlie", "35", "Chicago"],
        ],
    );
}

#[test]
fn test_select_where_and_or_mixed_parentheses_different() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT, city TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (1, 'Alice', 30, 'New York');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (2, 'Bob', 25, 'Los Angeles');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (3, 'Charlie', 35, 'Chicago');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age > 25 AND (name = 'Alice' OR city = 'Chicago');",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30", "New York"],
            vec!["3", "Charlie", "35", "Chicago"],
        ],
    );
}

#[test]
fn test_select_where_and_or_mixed_parentheses_override() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT, city TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (1, 'Alice', 30, 'New York');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (2, 'Bob', 25, 'Los Angeles');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (3, 'Charlie', 35, 'Chicago');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age > 25 OR name = 'Bob') AND city = 'Chicago';",
    );

    assert_rows_eq(result, vec![vec!["3", "Charlie", "35", "Chicago"]]);
}

#[test]
fn test_select_where_nested_parentheses_mixed() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT, city TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (1, 'Alice', 30, 'New York');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (2, 'Bob', 25, 'Los Angeles');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (3, 'Charlie', 35, 'Chicago');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age, city) VALUES (4, 'David', 40, 'Houston');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age > 25 AND (name = 'Alice' OR city = 'Chicago')) OR city = 'Houston';",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30", "New York"],
            vec!["3", "Charlie", "35", "Chicago"],
            vec!["4", "David", "40", "Houston"],
        ],
    );
}

#[test]
fn test_select_where_not_equal_and() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 30);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age = 30 AND name != 'Alice';",
    );

    assert_rows_eq(result, vec![vec!["3", "Charlie", "30"]]);
}

#[test]
fn test_select_where_not_equal_or() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age != 30 OR name = 'Bob';",
    );

    assert_rows_eq(
        result,
        vec![vec!["2", "Bob", "25"]],
    );
}

#[test]
fn test_select_where_greater_than_and() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age > 25 AND name = 'Alice';",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice", "30"]]);
}

#[test]
fn test_select_where_greater_than_or_equal_and() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age >= 30 AND name = 'Alice';",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice", "30"]]);
}

#[test]
fn test_select_where_less_than_and() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age < 30 AND name = 'Alice';",
    );

    assert_rows_eq(result, vec![]);
}

#[test]
fn test_select_where_less_than_or_equal_and() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age <= 30 AND name = 'Alice';",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice", "30"]]);
}

#[test]
fn test_select_where_greater_than_or_equal_or() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age >= 30 OR name = 'Bob';",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["2", "Bob", "25"]],
    );
}

#[test]
fn test_select_where_less_than_or_equal_or() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age <= 25 OR name = 'Alice';",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["2", "Bob", "25"]],
    );
}

#[test]
fn test_select_where_greater_than_or() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age > 30 OR name = 'Bob';",
    );

    assert_rows_eq(result, vec![vec!["2", "Bob", "25"]]);
}

#[test]
fn test_select_where_less_than_or() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age < 30 OR name = 'Alice';",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["2", "Bob", "25"]],
    );
}

#[test]
fn test_select_where_not_equal_and_or() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age != 25 AND name != 'Charlie';",
    );

    assert_rows_eq(result, vec![vec!["1", "Alice", "30"]]);
}

#[test]
fn test_select_where_greater_than_and_or() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age > 25 AND name = 'Alice' OR age > 30;",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["3", "Charlie", "35"]],
    );
}

#[test]
fn test_select_where_less_than_and_or() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age < 30 OR name = 'Alice' AND age = 30;",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["2", "Bob", "25"]],
    );
}

#[test]
fn test_select_where_greater_than_or_equal_and_or() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age >= 30 AND name = 'Alice' OR age >= 35;",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["3", "Charlie", "35"]],
    );
}

#[test]
fn test_select_where_less_than_or_equal_and_or() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age <= 25 OR name = 'Alice' AND age = 30;",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["2", "Bob", "25"]],
    );
}

#[test]
fn test_select_where_not_equal_and_or_mixed() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE age != 25 AND (name = 'Alice' OR name = 'Charlie');",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["3", "Charlie", "35"]],
    );
}

#[test]
fn test_select_where_greater_than_and_or_parentheses() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age > 25 AND name = 'Alice') OR (age > 30);",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["3", "Charlie", "35"]],
    );
}

#[test]
fn test_select_where_less_than_and_or_parentheses() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age < 30) OR (name = 'Alice' AND age = 30);",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["2", "Bob", "25"]],
    );
}

#[test]
fn test_select_where_greater_than_or_equal_and_or_parentheses() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age >= 30 AND name = 'Alice') OR (age >= 35);",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["3", "Charlie", "35"]],
    );
}

#[test]
fn test_select_where_less_than_or_equal_and_or_parentheses() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age <= 25) OR (name = 'Alice' AND age = 30);",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["2", "Bob", "25"]],
    );
}

#[test]
fn test_select_where_not_equal_and_or_parentheses() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age != 25) AND (name = 'Alice' OR name = 'Charlie');",
    );

    assert_rows_eq(
        result,
        vec![vec!["1", "Alice", "30"], vec!["3", "Charlie", "35"]],
    );
}

#[test]
fn test_select_where_greater_than_and_or_nested() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age > 25 AND (name = 'Alice' OR name = 'Charlie')) OR age > 35;",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["4", "David", "40"],
        ],
    );
}

#[test]
fn test_select_where_less_than_and_or_nested() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age < 40 AND (name = 'Alice' OR name = 'Charlie')) OR age < 30;",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["2", "Bob", "25"],
            vec!["3", "Charlie", "35"],
        ],
    );
}

#[test]
fn test_select_where_greater_than_or_equal_and_or_nested() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age >= 30 AND (name = 'Alice' OR name = 'Charlie')) OR age >= 40;",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["4", "David", "40"],
        ],
    );
}

#[test]
fn test_select_where_less_than_or_equal_and_or_nested() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age <= 35 AND (name = 'Alice' OR name = 'Charlie')) OR age <= 25;",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["2", "Bob", "25"],
            vec!["3", "Charlie", "35"],
        ],
    );
}

#[test]
fn test_select_where_not_equal_and_or_nested() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age != 25 AND (name = 'Alice' OR name = 'Charlie')) OR age != 40;",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["2", "Bob", "25"],
            vec!["3", "Charlie", "35"],
        ],
    );
}

#[test]
fn test_select_where_greater_than_and_or_nested_deep() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age > 25 AND (name = 'Alice' OR name = 'Charlie' OR name = 'Eve')) OR (age > 35 AND name = 'David');",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["4", "David", "40"],
            vec!["5", "Eve", "45"],
        ],
    );
}

#[test]
fn test_select_where_less_than_and_or_nested_deep() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age < 40 AND (name = 'Alice' OR name = 'Charlie')) OR (age < 30 AND name = 'Bob');",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["2", "Bob", "25"],
            vec!["3", "Charlie", "35"],
        ],
    );
}

#[test]
fn test_select_where_greater_than_or_equal_and_or_nested_deep() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age >= 30 AND (name = 'Alice' OR name = 'Charlie')) OR (age >= 40 AND name = 'David');",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["4", "David", "40"],
        ],
    );
}

#[test]
fn test_select_where_less_than_or_equal_and_or_nested_deep() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age <= 35 AND (name = 'Alice' OR name = 'Charlie')) OR (age <= 25 AND name = 'Bob');",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["2", "Bob", "25"],
            vec!["3", "Charlie", "35"],
        ],
    );
}

#[test]
fn test_select_where_not_equal_and_or_nested_deep() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age != 25 AND (name = 'Alice' OR name = 'Charlie')) OR (age != 40 AND name = 'David');",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
        ],
    );
}

#[test]
fn test_select_where_greater_than_and_or_nested_mixed() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age > 25 AND (name = 'Alice' OR name = 'Charlie')) OR (age > 35 AND (name = 'David' OR name = 'Eve'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["4", "David", "40"],
            vec!["5", "Eve", "45"],
        ],
    );
}

#[test]
fn test_select_where_less_than_and_or_nested_mixed() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age < 40 AND (name = 'Alice' OR name = 'Charlie')) OR (age < 30 AND (name = 'Bob'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["2", "Bob", "25"],
            vec!["3", "Charlie", "35"],
        ],
    );
}

#[test]
fn test_select_where_greater_than_or_equal_and_or_nested_mixed() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age >= 30 AND (name = 'Alice' OR name = 'Charlie')) OR (age >= 40 AND (name = 'David' OR name = 'Eve'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["4", "David", "40"],
            vec!["5", "Eve", "45"],
        ],
    );
}

#[test]
fn test_select_where_less_than_or_equal_and_or_nested_mixed() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age <= 35 AND (name = 'Alice' OR name = 'Charlie')) OR (age <= 25 AND (name = 'Bob'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["2", "Bob", "25"],
            vec!["3", "Charlie", "35"],
        ],
    );
}

#[test]
fn test_select_where_not_equal_and_or_nested_mixed() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age != 25 AND (name = 'Alice' OR name = 'Charlie')) OR (age != 40 AND (name = 'David' OR name = 'Eve'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["5", "Eve", "45"],
        ],
    );
}

#[test]
fn test_select_where_greater_than_and_or_nested_mixed_deep() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (6, 'Frank', 50);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age > 25 AND (name = 'Alice' OR name = 'Charlie' OR name = 'Eve')) OR (age > 35 AND (name = 'David' OR name = 'Frank'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["4", "David", "40"],
            vec!["5", "Eve", "45"],
            vec!["6", "Frank", "50"],
        ],
    );
}

#[test]
fn test_select_where_less_than_and_or_nested_mixed_deep() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (6, 'Frank', 50);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age < 40 AND (name = 'Alice' OR name = 'Charlie')) OR (age < 30 AND (name = 'Bob'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["2", "Bob", "25"],
            vec!["3", "Charlie", "35"],
        ],
    );
}

#[test]
fn test_select_where_greater_than_or_equal_and_or_nested_mixed_deep() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (6, 'Frank', 50);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age >= 30 AND (name = 'Alice' OR name = 'Charlie')) OR (age >= 40 AND (name = 'David' OR name = 'Frank'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["4", "David", "40"],
            vec!["6", "Frank", "50"],
        ],
    );
}

#[test]
fn test_select_where_less_than_or_equal_and_or_nested_mixed_deep() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (6, 'Frank', 50);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age <= 35 AND (name = 'Alice' OR name = 'Charlie')) OR (age <= 25 AND (name = 'Bob'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["2", "Bob", "25"],
            vec!["3", "Charlie", "35"],
        ],
    );
}

#[test]
fn test_select_where_not_equal_and_or_nested_mixed_deep() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (6, 'Frank', 50);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age != 25 AND (name = 'Alice' OR name = 'Charlie')) OR (age != 40 AND (name = 'David' OR name = 'Frank'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["6", "Frank", "50"],
        ],
    );
}

#[test]
fn test_select_where_greater_than_and_or_nested_mixed_deep_complex() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (6, 'Frank', 50);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (7, 'Grace', 55);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age > 25 AND (name = 'Alice' OR name = 'Charlie' OR name = 'Eve')) OR (age > 35 AND (name = 'David' OR name = 'Frank' OR name = 'Grace'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["4", "David", "40"],
            vec!["5", "Eve", "45"],
            vec!["6", "Frank", "50"],
            vec!["7", "Grace", "55"],
        ],
    );
}

#[test]
fn test_select_where_less_than_and_or_nested_mixed_deep_complex() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (6, 'Frank', 50);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (7, 'Grace', 55);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age < 40 AND (name = 'Alice' OR name = 'Charlie')) OR (age < 30 AND (name = 'Bob'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["2", "Bob", "25"],
            vec!["3", "Charlie", "35"],
        ],
    );
}

#[test]
fn test_select_where_greater_than_or_equal_and_or_nested_mixed_deep_complex() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (6, 'Frank', 50);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (7, 'Grace', 55);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age >= 30 AND (name = 'Alice' OR name = 'Charlie')) OR (age >= 40 AND (name = 'David' OR name = 'Frank' OR name = 'Grace'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["4", "David", "40"],
            vec!["6", "Frank", "50"],
            vec!["7", "Grace", "55"],
        ],
    );
}

#[test]
fn test_select_where_less_than_or_equal_and_or_nested_mixed_deep_complex() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (6, 'Frank', 50);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (7, 'Grace', 55);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age <= 35 AND (name = 'Alice' OR name = 'Charlie')) OR (age <= 25 AND (name = 'Bob'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["2", "Bob", "25"],
            vec!["3", "Charlie", "35"],
        ],
    );
}

#[test]
fn test_select_where_not_equal_and_or_nested_mixed_deep_complex() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (1, 'Alice', 30);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (2, 'Bob', 25);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (3, 'Charlie', 35);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (4, 'David', 40);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (5, 'Eve', 45);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (6, 'Frank', 50);",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name, age) VALUES (7, 'Grace', 55);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT * FROM users WHERE (age != 25 AND (name = 'Alice' OR name = 'Charlie')) OR (age != 40 AND (name = 'David' OR name = 'Frank' OR name = 'Grace'));",
    );

    assert_rows_eq(
        result,
        vec![
            vec!["1", "Alice", "30"],
            vec!["3", "Charlie", "35"],
            vec!["6", "Frank", "50"],
            vec!["7", "Grace", "55"],
        ],
    );
}

// =============================================================
// AGGREGATE TESTS
// =============================================================

#[test]
fn test_count_star() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (1, 'Alice');",
    );
    execute_sql(
        &mut executor,
        "INSERT INTO users (id, name) VALUES (2, 'Bob');",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT COUNT(*) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["2"]]);
}

#[test]
fn test_count_star_empty() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, name TEXT);",
    );

    let result = execute_sql(
        &mut executor,
        "SELECT COUNT(*) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["0"]]);
}

#[test]
fn test_sum() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 30);");

    let result = execute_sql(
        &mut executor,
        "SELECT SUM(age) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["60"]]);
}

#[test]
fn test_avg() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 30);");

    let result = execute_sql(
        &mut executor,
        "SELECT AVG(age) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["20"]]);
}

#[test]
fn test_min() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 30);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");

    let result = execute_sql(
        &mut executor,
        "SELECT MIN(age) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["10"]]);
}

#[test]
fn test_max() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 30);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");

    let result = execute_sql(
        &mut executor,
        "SELECT MAX(age) FROM users;",
    );

    assert_rows_eq(result, vec![vec!["30"]]);
}

// =============================================================
// GROUP BY TESTS
// =============================================================

#[test]
fn test_group_by_count() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (5, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (6, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (7, 40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, COUNT(*) FROM users GROUP BY age;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "2"],
            vec!["20", "2"],
            vec!["40", "3"],
        ],
    );
}

#[test]
fn test_group_by_sum() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 20);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, SUM(age) FROM users GROUP BY age;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "20"],
            vec!["20", "40"],
        ],
    );
}

#[test]
fn test_group_by_avg() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (5, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (6, 40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, AVG(age) FROM users GROUP BY age;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "10"],
            vec!["20", "20"],
            vec!["40", "40"],
        ],
    );
}

#[test]
fn test_group_by_min_integer() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (1,25);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (2,10);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (3,40);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (4,50);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (5,40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, MIN(age) FROM users GROUP BY age;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "10"],
            vec!["25", "25"],
            vec!["40", "40"],
            vec!["50", "50"],
        ],
    );
}

#[test]
fn test_group_by_max_integer() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (
            id INT PRIMARY KEY,
            age INT
        );",
    );

    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (1,25);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (2,10);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (3,40);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (4,50);");
    execute_sql(&mut executor, "INSERT INTO users (id,age) VALUES (5,40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, MAX(age) FROM users GROUP BY age;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "10"],
            vec!["25", "25"],
            vec!["40", "40"],
            vec!["50", "50"],
        ],
    );
}

#[test]
fn test_group_by_min_different_column() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE employees (
            id INT PRIMARY KEY,
            department TEXT,
            salary INT
        );",
    );

    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (1,'Engineering',100);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (2,'Engineering',250);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (3,'Engineering',180);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (4,'Sales',90);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (5,'Sales',150);");

    let result = execute_sql(
        &mut executor,
        "SELECT department, MIN(salary) FROM employees GROUP BY department;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["Engineering", "100"],
            vec!["Sales", "90"],
        ],
    );
}

#[test]
fn test_group_by_max_different_column() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE employees (
            id INT PRIMARY KEY,
            department TEXT,
            salary INT
        );",
    );

    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (1,'Engineering',100);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (2,'Engineering',250);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (3,'Engineering',180);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (4,'Sales',90);");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,salary) VALUES (5,'Sales',150);");

    let result = execute_sql(
        &mut executor,
        "SELECT department, MAX(salary) FROM employees GROUP BY department;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["Engineering", "250"],
            vec!["Sales", "150"],
        ],
    );
}

#[test]
fn test_group_by_min_text() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE employees (
            id INT PRIMARY KEY,
            department TEXT,
            name TEXT
        );",
    );

    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (1,'Engineering','Charlie');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (2,'Engineering','Alice');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (3,'Engineering','Bob');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (4,'Sales','David');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (5,'Sales','Aaron');");

    let result = execute_sql(
        &mut executor,
        "SELECT department, MIN(name) FROM employees GROUP BY department;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["Engineering", "Alice"],
            vec!["Sales", "Aaron"],
        ],
    );
}

#[test]
fn test_group_by_max_text() {

    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE employees (
            id INT PRIMARY KEY,
            department TEXT,
            name TEXT
        );",
    );

    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (1,'Engineering','Charlie');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (2,'Engineering','Alice');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (3,'Engineering','Bob');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (4,'Sales','David');");
    execute_sql(&mut executor, "INSERT INTO employees (id,department,name) VALUES (5,'Sales','Aaron');");

    let result = execute_sql(
        &mut executor,
        "SELECT department, MAX(name) FROM employees GROUP BY department;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["Engineering", "Charlie"],
            vec!["Sales", "David"],
        ],
    );
}

// =============================================================
// HAVING TESTS
// =============================================================

#[test]
fn test_having_count_greater_than() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (5, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (6, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (7, 40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, COUNT(*) FROM users GROUP BY age HAVING COUNT(*) > 1;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "2"],
            vec!["20", "2"],
            vec!["40", "3"],
        ],
    );
}

#[test]
fn test_having_sum_greater_than_or_equal() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, SUM(age) FROM users GROUP BY age HAVING SUM(age) >= 40;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["20", "40"],
        ],
    );
}

#[test]
fn test_having_avg_greater_than_or_equal() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, AVG(age) FROM users GROUP BY age HAVING AVG(age) >= 20;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["40", "40"],
        ],
    );
}

#[test]
fn test_having_min_greater_than() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 50);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (5, 40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, MIN(age) FROM users GROUP BY age HAVING MIN(age) > 20;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["40", "40"],
            vec!["50", "50"],
        ],
    );
}

#[test]
fn test_having_max_less_than() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 25);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 50);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (5, 40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, MAX(age) FROM users GROUP BY age HAVING MAX(age) < 45;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "10"],
            vec!["25", "25"],
            vec!["40", "40"],
        ],
    );
}

#[test]
fn test_having_no_matching_groups() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (5, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (6, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (7, 40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, COUNT(*) FROM users GROUP BY age HAVING COUNT(*) > 100;",
    );

    assert_rows_eq_sorted(result, vec![]);
}

#[test]
fn test_having_count_equal() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (5, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (6, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (7, 40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, COUNT(*) FROM users GROUP BY age HAVING COUNT(*) = 2;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "2"],
            vec!["20", "2"],
        ],
    );
}

#[test]
fn test_having_count_not_equal() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (5, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (6, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (7, 40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, COUNT(*) FROM users GROUP BY age HAVING COUNT(*) != 2;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["40", "3"],
        ],
    );
}

#[test]
fn test_having_count_greater_than_or_equal() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (5, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (6, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (7, 40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, COUNT(*) FROM users GROUP BY age HAVING COUNT(*) >= 2;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "2"],
            vec!["20", "2"],
            vec!["40", "3"],
        ],
    );
}

#[test]
fn test_having_count_less_than() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (5, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (6, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (7, 40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, COUNT(*) FROM users GROUP BY age HAVING COUNT(*) < 3;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "2"],
            vec!["20", "2"],
        ],
    );
}

#[test]
fn test_having_count_less_than_or_equal() {
    let mut executor = create_executor();

    execute_sql(
        &mut executor,
        "CREATE TABLE users (id INT PRIMARY KEY, age INT);",
    );

    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (1, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (2, 10);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (3, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (4, 20);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (5, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (6, 40);");
    execute_sql(&mut executor, "INSERT INTO users (id, age) VALUES (7, 40);");

    let result = execute_sql(
        &mut executor,
        "SELECT age, COUNT(*) FROM users GROUP BY age HAVING COUNT(*) <= 2;",
    );

    assert_rows_eq_sorted(
        result,
        vec![
            vec!["10", "2"],
            vec!["20", "2"],
        ],
    );
}