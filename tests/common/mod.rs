use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

use arch_db::engine::Engine;
use arch_db::sql::catalog::Catalog;
use arch_db::sql::executor::{Executor, QueryResult};
use arch_db::sql::lexer::Lexer;
use arch_db::sql::sql_parser::SQLParser;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A test-database helper that wraps the executor and provides a clean
/// SQL-based API for writing integration tests.
///
/// # Usage
///
/// ```ignore
/// let mut db = TestDB::new();
///
/// db.exec("CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);");
/// db.exec("INSERT INTO users VALUES (1, 'Alice', 30);");
/// db.exec("INSERT INTO users VALUES (2, 'Bob', 25);");
///
/// db.assert_query("SELECT * FROM users WHERE id = 1;", vec![vec!["1", "Alice", "30"]]);
/// db.assert_query("SELECT age FROM users WHERE age > 20;", vec![vec!["30"], vec!["25"]]);
/// db.assert_err("INSERT INTO users VALUES (3, 'abc');", "Type mismatch");
/// ```
pub struct TestDB {
    pub executor: Executor<'static>,
    storage_path: String,
}

impl TestDB {
    /// Create a new test database with an isolated storage directory.
    /// The storage directory is cleaned up on drop.
    pub fn new() -> Self {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = format!("storage/tests/test_db_{}", id);
        let _ = fs::remove_dir_all(&path);

        let engine = Box::leak(Box::new(Engine::with_storage_path(&path)));
        let catalog = Box::leak(Box::new(Catalog::new()));
        let executor = Executor::new(catalog, engine);

        Self {
            executor,
            storage_path: path,
        }
    }

    /// Parse and execute a SQL statement. Returns the raw `QueryResult`.
    ///
    /// Use this for statements where you want to inspect the result directly,
    /// or for statements where the return value doesn't matter.
    pub fn exec(&mut self, sql: &str) -> QueryResult {
        let lexer = Lexer::new(sql);
        let mut parser = SQLParser::new(lexer);
        let statement = parser.parse_statement();
        self.executor.execute(statement)
    }

    /// Execute a SQL query and return the result rows.
    ///
    /// Panics if the query does not return rows (e.g. it was an INSERT or UPDATE).
    pub fn query(&mut self, sql: &str) -> Vec<Vec<String>> {
        match self.exec(sql) {
            QueryResult::Rows(rows) => rows,
            other => panic!("Expected QueryResult::Rows, got {:?}", other),
        }
    }

    /// Execute a SQL query and assert that the result matches `expected`.
    pub fn assert_query(&mut self, sql: &str, expected: Vec<Vec<&str>>) {
        let result = self.query(sql);
        let expected: Vec<Vec<String>> = expected
            .into_iter()
            .map(|row| row.into_iter().map(|s| s.to_string()).collect())
            .collect();
        assert_eq!(
            result, expected,
            "SQL query failed.\n  Query: {}\n  Expected: {:?}\n  Got: {:?}",
            sql, expected, result
        );
    }

    /// Execute a SQL query, sort both actual and expected rows, then assert equality.
    pub fn assert_query_sorted(&mut self, sql: &str, expected: Vec<Vec<&str>>) {
        let mut result = self.query(sql);
        result.sort();
        let mut expected: Vec<Vec<String>> = expected
            .into_iter()
            .map(|row| row.into_iter().map(|s| s.to_string()).collect())
            .collect();
        expected.sort();
        assert_eq!(
            result, expected,
            "SQL query (sorted) failed.\n  Query: {}\n  Expected: {:?}\n  Got: {:?}",
            sql, expected, result
        );
    }

    /// Execute a SQL statement and assert that the result is a `Message` containing
    /// `expected_substring`. Use this for testing error cases.
    pub fn assert_err(&mut self, sql: &str, expected_substring: &str) {
        let result = self.exec(sql);
        match result {
            QueryResult::Message(msg) => {
                assert!(
                    msg.contains(expected_substring),
                    "Expected error message to contain '{}', but got: '{}'\n  Query: {}",
                    expected_substring,
                    msg,
                    sql
                );
            }
            other => {
                panic!(
                    "Expected QueryResult::Message containing '{}', but got {:?}\n  Query: {}",
                    expected_substring, other, sql
                );
            }
        }
    }

    /// Execute a SQL statement that returns a `Message` (e.g. INSERT, UPDATE, DELETE)
    /// and assert that the message is exactly `expected`.
    pub fn assert_message(&mut self, sql: &str, expected: &str) {
        let result = self.exec(sql);
        assert_eq!(
            result,
            QueryResult::Message(expected.to_string()),
            "SQL message assertion failed.\n  Query: {}\n  Expected: {}\n  Got: {:?}",
            sql,
            expected,
            result
        );
    }

    /// Execute an UPDATE or DELETE and assert the number of affected rows.
    pub fn assert_updated(&mut self, sql: &str, expected_rows: usize) {
        let expected = if expected_rows == 1 {
            "1 row updated".to_string()
        } else {
            format!("{} row(s) updated", expected_rows)
        };
        self.assert_message(sql, &expected);
    }

    /// Execute a DELETE and assert the number of deleted rows.
    pub fn assert_deleted(&mut self, sql: &str, expected_rows: usize) {
        let expected = if expected_rows == 1 {
            "1 row deleted".to_string()
        } else {
            let s = if expected_rows == 1 { "" } else { "s" };
            format!("{} row{} deleted", expected_rows, s)
        };
        self.assert_message(sql, &expected);
    }

    /// Assert that the query returns exactly `count` rows.
    pub fn assert_row_count(&mut self, sql: &str, count: usize) {
        let rows = self.query(sql);
        assert_eq!(
            rows.len(),
            count,
            "Expected {} row(s), got {}.\n  Query: {}\n  Rows: {:?}",
            count,
            rows.len(),
            sql,
            rows
        );
    }

    /// Assert that the query returns no rows.
    pub fn assert_empty(&mut self, sql: &str) {
        self.assert_row_count(sql, 0);
    }

    /// Assert that the query returns exactly one row.
    pub fn assert_one(&mut self, sql: &str) {
        self.assert_row_count(sql, 1);
    }
}

impl Drop for TestDB {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.storage_path);
    }
}