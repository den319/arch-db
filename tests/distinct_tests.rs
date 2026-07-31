mod common;
use common::TestDB;

/// Verify basic DISTINCT on a single column with duplicate values.
#[test]
fn test_distinct_basic() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, department TEXT);");
    db.exec("INSERT INTO users VALUES (1, 'Engineering');");
    db.exec("INSERT INTO users VALUES (2, 'Engineering');");
    db.exec("INSERT INTO users VALUES (3, 'Sales');");
    db.exec("INSERT INTO users VALUES (4, 'Sales');");
    db.exec("INSERT INTO users VALUES (5, 'HR');");

    // DISTINCT should return 3 unique departments
    db.assert_query_sorted(
        "SELECT DISTINCT department FROM users;",
        vec![
            vec!["Engineering"],
            vec!["HR"],
            vec!["Sales"],
        ],
    );

    // Non-DISTINCT should return all 5 rows
    db.assert_row_count("SELECT department FROM users;", 5);
}

/// Verify DISTINCT with multiple columns.
#[test]
fn test_distinct_multiple_columns() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, department TEXT, city TEXT);");
    db.exec("INSERT INTO users VALUES (1, 'Engineering', 'NYC');");
    db.exec("INSERT INTO users VALUES (2, 'Engineering', 'NYC');");
    db.exec("INSERT INTO users VALUES (3, 'Engineering', 'SF');");
    db.exec("INSERT INTO users VALUES (4, 'Sales', 'NYC');");
    db.exec("INSERT INTO users VALUES (5, 'Sales', 'NYC');");

    // DISTINCT on (department, city) should return 3 unique combinations
    db.assert_query_sorted(
        "SELECT DISTINCT department, city FROM users;",
        vec![
            vec!["Engineering", "NYC"],
            vec!["Engineering", "SF"],
            vec!["Sales", "NYC"],
        ],
    );
}

/// Verify DISTINCT with WHERE clause.
#[test]
fn test_distinct_with_where() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, department TEXT, age INT);");
    db.exec("INSERT INTO users VALUES (1, 'Engineering', 30);");
    db.exec("INSERT INTO users VALUES (2, 'Engineering', 30);");
    db.exec("INSERT INTO users VALUES (3, 'Engineering', 25);");
    db.exec("INSERT INTO users VALUES (4, 'Sales', 30);");
    db.exec("INSERT INTO users VALUES (5, 'Sales', 25);");

    // DISTINCT with WHERE
    db.assert_query_sorted(
        "SELECT DISTINCT department FROM users WHERE age = 30;",
        vec![
            vec!["Engineering"],
            vec!["Sales"],
        ],
    );
}

/// Verify DISTINCT with ORDER BY.
#[test]
fn test_distinct_with_order_by() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, department TEXT);");
    db.exec("INSERT INTO users VALUES (1, 'Engineering');");
    db.exec("INSERT INTO users VALUES (2, 'Sales');");
    db.exec("INSERT INTO users VALUES (3, 'Engineering');");
    db.exec("INSERT INTO users VALUES (4, 'HR');");

    // DISTINCT with ORDER BY ASC
    db.assert_query(
        "SELECT DISTINCT department FROM users ORDER BY department ASC;",
        vec![
            vec!["Engineering"],
            vec!["HR"],
            vec!["Sales"],
        ],
    );

    // DISTINCT with ORDER BY DESC
    db.assert_query(
        "SELECT DISTINCT department FROM users ORDER BY department DESC;",
        vec![
            vec!["Sales"],
            vec!["HR"],
            vec!["Engineering"],
        ],
    );
}

/// Verify DISTINCT with LIMIT.
#[test]
fn test_distinct_with_limit() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, department TEXT);");
    db.exec("INSERT INTO users VALUES (1, 'Engineering');");
    db.exec("INSERT INTO users VALUES (2, 'Engineering');");
    db.exec("INSERT INTO users VALUES (3, 'Sales');");
    db.exec("INSERT INTO users VALUES (4, 'Sales');");
    db.exec("INSERT INTO users VALUES (5, 'HR');");

    // DISTINCT with LIMIT should return at most N unique rows
    db.assert_row_count("SELECT DISTINCT department FROM users LIMIT 2;", 2);
}

/// Verify DISTINCT on an empty table returns no rows.
#[test]
fn test_distinct_empty() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, department TEXT);");

    db.assert_empty("SELECT DISTINCT department FROM users;");
}

/// Verify DISTINCT when all rows are already unique (no duplicates).
#[test]
fn test_distinct_all_unique() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, name TEXT);");
    db.exec("INSERT INTO users VALUES (1, 'Alice');");
    db.exec("INSERT INTO users VALUES (2, 'Bob');");
    db.exec("INSERT INTO users VALUES (3, 'Charlie');");

    // All rows are unique, so DISTINCT should return the same as non-DISTINCT
    db.assert_query_sorted(
        "SELECT DISTINCT name FROM users;",
        vec![
            vec!["Alice"],
            vec!["Bob"],
            vec!["Charlie"],
        ],
    );
}

/// Verify DISTINCT with integer column.
#[test]
fn test_distinct_integer() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, age INT);");
    db.exec("INSERT INTO users VALUES (1, 25);");
    db.exec("INSERT INTO users VALUES (2, 30);");
    db.exec("INSERT INTO users VALUES (3, 25);");
    db.exec("INSERT INTO users VALUES (4, 30);");
    db.exec("INSERT INTO users VALUES (5, 35);");

    db.assert_query_sorted(
        "SELECT DISTINCT age FROM users;",
        vec![
            vec!["25"],
            vec!["30"],
            vec!["35"],
        ],
    );
}

/// Verify that non-DISTINCT queries still work correctly (regression test).
#[test]
fn test_non_distinct_still_works() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, department TEXT);");
    db.exec("INSERT INTO users VALUES (1, 'Engineering');");
    db.exec("INSERT INTO users VALUES (2, 'Engineering');");

    // Non-DISTINCT should return all rows including duplicates
    db.assert_row_count("SELECT department FROM users;", 2);
}

/// Verify DISTINCT with primary key (always unique, so DISTINCT is a no-op).
#[test]
fn test_distinct_on_primary_key() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, name TEXT);");
    db.exec("INSERT INTO users VALUES (1, 'Alice');");
    db.exec("INSERT INTO users VALUES (2, 'Bob');");
    db.exec("INSERT INTO users VALUES (3, 'Alice');");

    // DISTINCT on primary key should return all rows since PK is unique
    db.assert_row_count("SELECT DISTINCT id FROM users;", 3);
}

/// Verify DISTINCT with SELECT * (all columns).
#[test]
fn test_distinct_select_all() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, department TEXT, age INT);");
    db.exec("INSERT INTO users VALUES (1, 'Engineering', 30);");
    db.exec("INSERT INTO users VALUES (2, 'Engineering', 30);");  // duplicate of id=1
    db.exec("INSERT INTO users VALUES (3, 'Engineering', 25);");
    db.exec("INSERT INTO users VALUES (4, 'Sales', 30);");

    // SELECT DISTINCT * should return 3 unique rows (id=1 and id=2 differ by PK)
    db.assert_row_count("SELECT DISTINCT * FROM users;", 4);
}