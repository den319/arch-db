# ArchDB — Testing Philosophy & Requirements

This file defines the testing standards for the ArchDB project. Every feature must be tested thoroughly before being marked as complete.

---

## Core Principle: Test Interactions, Not Just Functions

Nearly every significant bug in ArchDB has been caused by **interactions between correct components**, not by a faulty individual function. For example:
- `get_key()` returned the correct `Option<Value>` but the callers handled `Some(Value::Tombstone)` and `None` incorrectly.
- Type validation was correct on read but missing on write.
- The fast path worked correctly for found keys but returned empty for missing keys instead of falling through.

Therefore, tests must verify **end-to-end behavior** across component boundaries, not just isolated unit tests.

---

## Integration Test Harness: `TestDB`

To make writing integration tests as easy as writing SQL, use the `TestDB` helper in `tests/test_harness.rs`.

### Quick Start

```rust
use test_harness::TestDB;

#[test]
fn test_example() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, name TEXT, age INT);");
    db.exec("INSERT INTO users VALUES (1, 'Alice', 30);");
    db.exec("INSERT INTO users VALUES (2, 'Bob', 25);");

    db.assert_query("SELECT * FROM users WHERE id = 1;", vec![
        vec!["1", "Alice", "30"],
    ]);
}
```

### Available Methods

| Method | Purpose | Example |
|--------|---------|---------|
| `exec(sql)` | Execute any SQL, return `QueryResult` | `db.exec("INSERT ...");` |
| `query(sql)` | Execute SELECT, return `Vec<Vec<String>>` | `let rows = db.query("SELECT ...");` |
| `assert_query(sql, expected)` | SELECT + assert exact rows | `db.assert_query("SELECT ...", vec![...]);` |
| `assert_query_sorted(sql, expected)` | SELECT + sort both + assert | `db.assert_query_sorted("SELECT ...", vec![...]);` |
| `assert_err(sql, substring)` | Assert error message contains text | `db.assert_err("INSERT ...", "Type mismatch");` |
| `assert_message(sql, expected)` | Assert exact message | `db.assert_message("INSERT ...", "Insert parsed successfully");` |
| `assert_updated(sql, n)` | Assert UPDATE affected n rows | `db.assert_updated("UPDATE ...", 1);` |
| `assert_deleted(sql, n)` | Assert DELETE affected n rows | `db.assert_deleted("DELETE ...", 1);` |
| `assert_row_count(sql, n)` | Assert SELECT returns n rows | `db.assert_row_count("SELECT ...", 5);` |
| `assert_empty(sql)` | Assert SELECT returns 0 rows | `db.assert_empty("SELECT ...");` |
| `assert_one(sql)` | Assert SELECT returns 1 row | `db.assert_one("SELECT ...");` |

### Testing Type Mismatches

```rust
#[test]
fn test_type_mismatch_on_insert() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, age INT);");

    // Inserting a string into an INTEGER column must fail
    db.assert_err(
        "INSERT INTO users VALUES (1, 'abc');",
        "Type mismatch",
    );
}
```

### Testing Error Cases

```rust
#[test]
fn test_missing_key() {
    let mut db = TestDB::new();

    db.exec("CREATE TABLE users (id INT PRIMARY KEY, name TEXT);");
    db.exec("INSERT INTO users VALUES (1, 'Alice');");

    // Querying a non-existent key returns empty
    db.assert_empty("SELECT * FROM users WHERE id = 999;");

    // Updating a non-existent key returns 0 rows
    db.assert_updated("UPDATE users SET name = 'Bob' WHERE id = 999;", 0);
}
```

---

## Unit Test Requirements

### 1. Test Every Return Path
Every function that returns a `Result` or `Option` must have tests for:
- **Success case** — the happy path returns `Ok(...)` or `Some(...)`
- **Error case** — each error variant returns the expected error message
- **Edge case** — empty inputs, boundary values, type mismatches

### 2. Test Every Match Arm
Every `match` expression with multiple arms should have a test for each arm:
- `Some(Value::Data(...))` — key exists with data
- `Some(Value::Tombstone)` — key exists but was deleted
- `None` — key does not exist

### 3. Test Storage Layer (SSTable + Memtable)
Tests must exercise both:
- **Memtable-only path** — data is in memory (small datasets, single session)
- **SSTable path** — data is flushed to disk (force flush, shutdown/restart)
- **Mixed path** — data spans both memtable and SSTables

### 4. Test Cross-Type Operations
- Inserting a string into an INTEGER column must fail
- Inserting an integer into a TEXT column must fail
- WHERE clause with mismatched types must produce a clear error (not a silent failure)
- Primary key lookup with mismatched types must produce a clear error

### 5. Test Bloom Filter Interactions
- Verify that keys are found even when bloom filter has false positives
- Verify that non-existent keys return `None` (not `Some(Value::Tombstone)`)

### 6. Test the Fast Path and Fallback Path
For SELECT, UPDATE, DELETE:
- **Fast path** (primary key lookup) — verify it finds the correct row
- **Fast path with missing key** — verify it falls through to table scan (does not return empty)
- **Fallback path** (table scan) — verify it works when fast path is not applicable

### 7. Test Data Persistence
- Insert data, shutdown, restart, and verify the data is still accessible
- This exercises WAL replay, manifest recovery, and SSTable loading

---

## Integration / Regression Testing

### Why Integration Tests Matter
Unit tests verify individual functions. But bugs often live at the boundaries between components:
- The SQL executor calls `engine.get()`, which searches SSTables, which uses bloom filters, which may have false positives.
- The bloom filter says "maybe" → the SSTable is searched → the key is not found → `get_key()` returns `None` or `Tombstone`.
- The executor must handle all three outcomes correctly.

### How to Write Integration Tests
1. **Start from SQL** — Use `TestDB::exec()` to go through the full pipeline (parser → executor → engine → storage).
2. **Verify the output** — Use `assert_query()` for SELECT results, `assert_message()` for errors.
3. **Test the error path** — Don't just test that the happy path works. Test what happens when things go wrong.
4. **Test persistence** — Shut down the engine and create a new one, then verify the data is still accessible.

### Example: Integration Test Pattern

```rust
#[test]
fn test_full_integration() {
    let mut db = TestDB::new();

    // Setup
    db.exec("CREATE TABLE test (id INT PRIMARY KEY, value INT);");

    // Happy path
    db.exec("INSERT INTO test VALUES (1, 100);");
    db.assert_query("SELECT * FROM test WHERE id = 1;", vec![vec!["1", "100"]]);

    // Error path — type mismatch
    db.assert_err("INSERT INTO test VALUES (2, 'abc');", "Type mismatch");

    // Edge case — missing key
    db.assert_empty("SELECT * FROM test WHERE id = 999;");

    // Update
    db.assert_updated("UPDATE test SET value = 200 WHERE id = 1;", 1);
    db.assert_query("SELECT value FROM test WHERE id = 1;", vec![vec!["200"]]);

    // Delete
    db.assert_deleted("DELETE FROM test WHERE id = 1;", 1);
    db.assert_empty("SELECT * FROM test WHERE id = 1;");
}
```

---

## Bug Prevention Checklist

Before marking a feature as complete, verify:

- [ ] All existing tests pass
- [ ] New tests cover the feature's success path
- [ ] New tests cover the feature's error paths
- [ ] New tests cover type mismatches (if applicable)
- [ ] New tests cover both memtable and SSTable paths
- [ ] New tests cover the fast path AND fallback path
- [ ] The feature does not silently return incorrect results
- [ ] Error messages are clear and actionable
- [ ] Data written by the feature survives a shutdown/restart cycle

---

## Test File Organization

| Test File | What It Tests |
|-----------|---------------|
| `tests/test_harness.rs` | `TestDB` helper — shared test infrastructure |
| `tests/executor_tests.rs` | SQL executor: INSERT, SELECT, UPDATE, DELETE, WHERE, ORDER BY, LIMIT, DISTINCT, type validation |
| `tests/executor_tests_1.rs` | Additional executor tests (GROUP BY, HAVING, aggregates) |
| `tests/parser_tests.rs` | SQL parser: all statement types, expressions, edge cases |
| `tests/lexer_tests.rs` | Tokenizer: all token types, whitespace, comments |
| `tests/row_tests.rs` | Row serialization/deserialization |
| `tests/catalog_tests.rs` | Catalog: table/index creation, schema serialization |
| `tests/table_tests.rs` | Table: storage key generation, index encoding |
| `tests/engine_tests.rs` | Engine: put, get, delete, scan, flush, compaction |
| `tests/sstable_tests.rs` | SSTable: write, read, search, index, bloom filter |
| `tests/sstable_manager_tests.rs` | SSTable manager: manifest, compaction, recovery |

---

## How to Write a Test (Example Pattern)

```rust
#[test]
fn test_feature_name() {
    let mut db = TestDB::new();

    // Create table
    db.exec("CREATE TABLE test (id INT PRIMARY KEY, value INT);");

    // Insert data
    db.exec("INSERT INTO test VALUES (1, 100);");

    // Test success case
    db.assert_query("SELECT * FROM test WHERE id = 1;", vec![vec!["1", "100"]]);

    // Test error case
    db.assert_err("INSERT INTO test VALUES (2, 'abc');", "Type mismatch");

    // Test edge case
    db.assert_empty("SELECT * FROM test WHERE id = 999;");
}