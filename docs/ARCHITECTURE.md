# ArchDB Architecture

## Overview

ArchDB is an LSM-tree (Log-Structured Merge-Tree) database engine written in Rust. It supports a subset of SQL and provides persistent, crash-safe storage with configurable write-ahead logging.

## Directory Structure

```
src/
├── lib.rs              # Module declarations
├── main.rs             # CLI entry point (raw Command interface)
├── engine.rs           # Core Engine: memtable, flush, compaction, get/put/delete/scan/iter, range_scan
├── engine_iterator.rs  # Wraps UnifiedStorageIterator for Engine.iter()
├── memtable_iterator.rs# Iterator over BTreeMap<String, Value>
├── merge_iterator.rs   # K-way merge of multiple StorageIterators (with dedup) — deprecated
├── range_iterator.rs   # Prefix range scan iterator (used by Engine.range_scan())
├── unified_storage_iterator.rs # Single iterator over memtable + all SSTables
├── storage_iterator.rs # StorageIterator trait definition
├── sstable.rs          # SSTable read/write, block compression, binary search, bloom filter integration
├── sstable_manager.rs  # SSTable lifecycle: add, remove, compact, manifest, levels
├── compaction_picker.rs# Picks candidate tables for compaction
├── cache.rs            # LRU block cache
├── bloom_filter.rs     # Bloom filter implementation
├── command.rs          # Raw CLI commands (Set, Get, Del, Scan, etc.)
├── parser.rs           # Raw CLI command parser (legacy)
├── storage.rs          # WAL (Write-Ahead Log) with CRC, rotation, recovery
├── error.rs            # DatabaseError enum and Result type
├── helper.rs           # Utility functions
├── schema.rs           # Schema definitions
└── sql/
    ├── mod.rs          # SQL module declarations
    ├── token.rs        # SQL token types (including PRIMARY, KEY, CREATE INDEX, ON)
    ├── lexer.rs        # SQL tokenizer
    ├── sql_parser.rs   # SQL → AST parser (supports PRIMARY KEY, CREATE INDEX syntax)
    ├── ast.rs          # SQL AST node types (ColumnDef, CreateIndex, Statement)
    ├── executor.rs     # SQL statement executor (CREATE TABLE, INSERT, SELECT, UPDATE, DELETE, CREATE INDEX)
    ├── expression.rs   # WHERE clause expression evaluator
    ├── catalog.rs      # Table schema + index schema metadata store
    ├── planner.rs      # Query planner: IndexLookup struct for index-scan plans
    ├── row.rs          # Row type with serialization/deserialization
    └── table.rs        # Table helper: storage key generation, primary key handling
```

## Storage Engine Architecture

### Write Path
```
Client → Engine.put(key, value)
         → Memtable (BTreeMap)
         → maybe_flush() if memtable_limit reached
           → flush_to_sstable() → SSTable file on disk
           → Manifest updated
           → Background compaction triggered
```

### Read Path (Point Lookup)
```
Client → Engine.get(key)
         → Check Memtable first
         → Lock SSTableManager → check Bloom filters → collect candidates
         → Unlock (no lock held during disk I/O)
         → For each candidate SSTable (newest first):
           → Check Block Cache
           → Cache miss → read block from disk → cache it
           → Binary search within block
         → Return Value or Tombstone
```

### Read Path (Full Scan)
```
Client → Engine.iter()
         → Create UnifiedStorageIterator:
           → MemtableIterator (memtable snapshot)
           → SSTableIterator for each SSTable (L0, L1, L2, newest first)
         → UnifiedStorageIterator merges all iterators in key order
```

### Read Priority

This is one of the most important rules of an LSM database. When reading a key, the first visible version wins:

1. **Memtable** — most recent writes (checked first)
2. **L0 SSTables** — newest table first (reverse iteration)
3. **L1 SSTables** — newest table first
4. **L2 SSTables** — newest table first

**Key rules:**
- The first visible version of a key wins (newest data takes priority)
- A `Tombstone` value means the key was deleted — it stops the search and returns "not found"
- Bloom filters are checked before reading any SSTable to avoid unnecessary disk I/O
- The lock on SSTableManager is released before any disk reads, so compaction is not blocked

### Compaction

- **Size-tiered compaction**: When a level exceeds a threshold, tables are merged into the next level
- **L0 → L1**: Triggered by CompactionPicker when L0 has enough tables
- **L1 → L2**: Triggered when L1 exceeds threshold
- **Background worker**: Async compaction via mpsc channel; the SSTableManager lock is released during disk I/O so writes are not blocked
- **SSTable splitting**: Large compaction outputs are split into multiple files to avoid oversized tables
- **Duplicate key handling**: Compaction preserves the newest version of duplicate keys and drops obsolete versions
- **Tombstone preservation**: Tombstones are preserved during compaction until it is safe to remove them (i.e., all older data that they shadow has been compacted away)

### WAL (Write-Ahead Log)

- Every write (Set/Del) is appended to the WAL before being applied to the memtable
- CRC32 checksums detect corruption
- WAL segments are rotated after flush
- On startup, WAL is replayed to restore memtable state
- SyncPolicy controls durability (Always, Every N seconds, etc.)

## SQL Layer Architecture

### Query Execution Flow
```
SQL String → Lexer → Token Stream → Parser → AST → Executor → QueryResult
```

### CREATE TABLE Flow

```
CREATE TABLE users (
    id INT PRIMARY KEY,
    name TEXT
);
```

1. **Parser** parses each column definition. After parsing the data type, it checks for optional `PRIMARY KEY` keywords.
2. **Validation** (`execute_create_table`): Exactly one column must have `primary_key: true`. Rejects 0 or multiple PKs with descriptive errors.
3. **Schema construction**: Each `ColumnDef` is converted to a `Column` with `primary_key` and `nullable` flags.
4. **Catalog registration**: Schema is stored in the in-memory catalog and persisted to the storage engine with a `__schema__:` prefix key.

### CREATE INDEX Flow

```sql
CREATE INDEX idx_users_name ON users (name);
```

1. **Parser** (`parse_create_index`): After `CREATE` token, if the next token is `INDEX`, it parses: index name, `ON`, table name, `(column_name)`.
2. **Validation** (`execute_create_index`): Validates the table exists and the column exists in the table schema.
3. **Metadata persistence**: An `IndexSchema` (name, table_name, column_name) is serialized and stored with a `__index_meta__:{name}` key.
4. **Catalog registration**: The `IndexSchema` is added to `Catalog.indexes` (in-memory `HashMap<String, IndexSchema>`).
5. **Physical index build** (`build_index`): Scans every row in the table, constructs an index storage key for each row, and writes it to the engine.

### Executor Flow (SELECT)

```
execute_select(stmt)
  ├─ Lookup table schema from Catalog
  │
  ├─ FAST PATH (Primary Key Lookup):
  │   Condition: WHERE clause is `primary_key = <literal>`
  │   → Identifies PK column via `table.schema.primary_key()` (not first-column assumption)
  │   → Direct key lookup via Engine.get()
  │   → O(log n) — single key lookup
  │   → Deserialize row, project columns, return
  │
  ├─ INDEX SCAN PATH:
  │   Condition: WHERE clause matches an indexed column with a supported operator (=, >, >=, <, <=)
  │   → `find_usable_index()` checks catalog for an index on the WHERE column
  │   → If found, returns `IndexLookup { column, operator, value }`
  │   → `build_index_range()` converts `IndexLookup` into (start, end) key bounds
  │   → `range_scan()` on `__index__:{table}:{column}:{encoded_value}*` prefix
  │   → `fetch_rows_by_primary_keys()` fetches full rows from storage engine
  │   → Post-filter with `ExpressionEvaluator` for correctness
  │   → Return matched rows
  │
  └─ FULL TABLE SCAN (fallback):
      Condition: No usable index found, no WHERE clause, or complex expressions
      → Full scan via Engine.iter() over all data (memtable + all SSTables)
      → For each row matching the table's key prefix:
        → Evaluate WHERE clause via ExpressionEvaluator
        → Project requested columns
        → Collect matching rows
      → Return
```

### Executor Flow (DELETE)

```
execute_delete(stmt)
  ├─ Lookup table schema from Catalog
  ├─ Require WHERE clause (DELETE without WHERE is rejected)
  │
  ├─ FAST PATH (Primary Key Lookup):
  │   Condition: WHERE clause is `primary_key = <literal>`
  │   → Direct key lookup via Engine.get()
  │   → Remove index entries for the old row
  │   → Write Tombstone via Engine.delete()
  │   → Return "1 row deleted"
  │
  ├─ INDEX SCAN PATH:
  │   Condition: WHERE clause matches an indexed column
  │   → Same index lookup flow as SELECT
  │   → Remove index entries for each matched row
  │   → Write Tombstone for each matched key
  │   → Return "{n} row(s) deleted"
  │
  └─ FULL TABLE SCAN (fallback):
      Condition: No usable index, complex WHERE expressions
      → Full scan via Engine.iter() over all data
      → For each row matching the table's key prefix:
        → Evaluate WHERE clause via ExpressionEvaluator
        → Collect matching keys
      → Remove index entries for each matching row
      → Write Tombstone for each matching key via Engine.delete()
      → Return "{n} row(s) deleted"
```

### Executor Flow (UPDATE)

```
execute_update(stmt)
  ├─ Lookup table schema from Catalog
  ├─ Require WHERE clause (UPDATE without WHERE is rejected)
  │
  ├─ FAST PATH (Primary Key Lookup):
  │   Condition: WHERE clause is `primary_key = <literal>`
  │   → Direct key lookup via Engine.get()
  │   → Apply SET assignments to deserialized row
  │   → Rejects attempts to modify the PK column
  │   → Remove old index entries for the old row
  │   → Re-serialize and write back via Engine.put()
  │   → Insert new index entries for the updated row
  │   → Return "1 row updated"
  │
  ├─ INDEX SCAN PATH:
  │   Condition: WHERE clause matches an indexed column
  │   → Same index lookup flow as SELECT
  │   → For each matched row:
  │     → Apply SET assignments
  │     → Reject PK modification
  │     → Remove old index entries
  │     → Write updated row
  │     → Insert new index entries
  │   → Return "{n} row(s) updated"
  │
  └─ FULL TABLE SCAN (fallback):
      Condition: No usable index, complex WHERE expressions
      → Full scan via Engine.iter() over all data
      → For each row matching the table's key prefix:
        → Evaluate WHERE clause via ExpressionEvaluator
        → Apply SET assignments to matching rows
        → Rejects attempts to modify the PK column
        → Remove old index entries for the old row
        → Re-serialize and write back via Engine.put()
        → Insert new index entries for the updated row
      → Return "{n} row(s) updated"
```

### Index Maintenance

Index entries are maintained on all data-modifying operations:

- **INSERT** (`execute_insert`): After writing the row, calls `insert_index_entries()` which iterates over all indexes for the table and writes a `__index__:{table}:{column}:{value}:{pk}` key for each.
- **DELETE** (`execute_delete`): Before writing the tombstone, calls `delete_index_entries()` to remove all index entries for the row being deleted.
- **UPDATE** (`execute_update`): Removes old index entries for the original row (via `delete_index_entries`), then writes the updated row, then inserts new index entries for the updated values (via `insert_index_entries`).

### Index Storage Key Format

```
__index__:{table_name}:{column_name}:{column_value}:{primary_key_value}
```

Example: `__index__:users:name:Alice:42`

This format enables:
- Efficient lookup of all rows matching a given index value via prefix scan (`__index__:users:name:Alice:`)
- Uniqueness per (table, column, value, pk) combination
- Easy cleanup on DELETE by regenerating the exact key

### Index Metadata Persistence

Index metadata (`IndexSchema`) is serialized and stored with a `__index_meta__:{name}` key in the storage engine. On startup, `Catalog.load_indexes_from_engine()` scans all `__index_meta__:` prefixed keys and reconstructs the index catalog.

### Key Data Structures

```rust
// Storage engine value type
enum Value {
    Data(String),
    Tombstone,
}

// SQL row value type
enum RowValue {
    Integer(i64),
    Text(String),
}

// Query result
enum QueryResult {
    None,
    Message(String),
    Rows(Vec<Vec<String>>),
}

// Error handling
enum DatabaseError {
    Io(io::Error),
    Other(String),
}

// Column definition (AST)
struct ColumnDef {
    name: String,
    data_type: DataType,
    primary_key: bool,  // false by default; true if PRIMARY KEY specified
}

// Column (catalog schema)
struct Column {
    name: String,
    data_type: CatalogDataType,
    primary_key: bool,
    nullable: bool,  // false for PK columns, true for non-PK
}

// Index schema (catalog)
struct IndexSchema {
    name: String,
    table_name: String,
    column_name: String,
}
```

### Storage Key Format
```
{table_name}:{primary_key_value}
```
Example: `"users:42"`, `"products:abc123"`

### Index Storage Key Format
```
__index__:{table_name}:{column_name}:{value}:{pk}
```
Example: `__index__:users:name:Alice:42`

### Index Metadata Key Format
```
__index_meta__:{index_name}
```
Example: `__index_meta__:idx_users_name`

### Row Serialization Format
```
"column1=i:42|column2=t:hello|column3=t:world"
```
- `i:` prefix for integers
- `t:` prefix for text
- `|` separator between columns
- Columns are stored in BTreeMap order (alphabetical) in the serialized format

### Primary Key Persistence

The catalog schema is serialized to and deserialized from the storage engine (via `__schema__:table_name` keys). Currently, the `primary_key` flag is **not** preserved in serialization — the `serialize()` and `deserialize()` methods in `catalog.rs` only encode column names and types. On restart, all columns are loaded with `primary_key: false`. This means PK metadata is only valid for the current session. This should be addressed in a future update.

### Column Ordering in Query Results

When projecting rows with `SELECT *` (Wildcard), columns are returned in the order they were defined in `CREATE TABLE`, not in alphabetical order. This is achieved by iterating over the schema's column list rather than the row's internal BTreeMap. For explicit column selection (`SELECT col1, col2`), columns are returned in the order specified in the query.

## Iterator Hierarchy

```
StorageIterator (trait)
├── MemtableIterator — iterates over BTreeMap<String, Value>
├── SSTableIterator — iterates over a single SSTable file
└── UnifiedStorageIterator — merges multiple StorageIterators
    (used by Engine.iter() for full table scans)
```

## Current Limitations

- No transaction support
- CLI uses raw Command interface, not SQL
- MergeIterator is no longer used (replaced by UnifiedStorageIterator) — should be removed
- Parser panics on syntax errors instead of returning Result
- memtable_limit is hardcoded to 1000
- Block cache size is hardcoded to 64 blocks
- Primary key flag (`primary_key: bool`) is not persisted in catalog schema serialization (lost on restart)
- Index range scans support equality and comparison operators (>, >=, <, <=) but not `!=` or `LIKE`
- `find_usable_index()` only supports `column <op> literal` patterns — complex boolean expressions with AND/OR are not yet routed through the index planner
- Cost-based query optimization not yet implemented (always uses index scan if available, falls back to table scan)
