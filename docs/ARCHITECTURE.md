# ArchDB Architecture

## Overview

ArchDB is an LSM-tree (Log-Structured Merge-Tree) database engine written in Rust. It supports a subset of SQL and provides persistent, crash-safe storage with configurable write-ahead logging.

## Directory Structure

```
src/
├── lib.rs              # Module declarations
├── main.rs             # CLI entry point (raw Command interface)
├── engine.rs           # Core Engine: memtable, flush, compaction, get/put/delete/scan/iter
├── engine_iterator.rs  # Wraps UnifiedStorageIterator for Engine.iter()
├── memtable_iterator.rs# Iterator over BTreeMap<String, Value>
├── merge_iterator.rs   # K-way merge of multiple StorageIterators (with dedup)
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
    ├── token.rs        # SQL token types
    ├── lexer.rs        # SQL tokenizer
    ├── parser.rs       # SQL → AST parser
    ├── ast.rs          # SQL AST node types
    ├── executor.rs     # SQL statement executor (CREATE, INSERT, SELECT, UPDATE, DELETE)
    ├── expression.rs   # WHERE clause expression evaluator
    ├── catalog.rs      # Table schema metadata store
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

### Executor Flow (SELECT)

```
execute_select(stmt)
  ├─ Lookup table schema from Catalog
  │
  ├─ FAST PATH (Primary Key Lookup):
  │   Condition: WHERE clause is `primary_key = <literal>`
  │   → Direct key lookup via Engine.get()
  │   → O(log n) — single key lookup
  │   → Deserialize row, project columns, return
  │
  └─ SLOW PATH (Full Table Scan):
      Condition: Everything else (no WHERE, non-PK WHERE, range conditions)
      → Full scan via Engine.iter() over all data (memtable + all SSTables)
      → For each row matching the table's key prefix:
        → Evaluate WHERE clause via ExpressionEvaluator
        → Project requested columns
        → Collect matching rows
      → Return
```

This optimization is significant: primary key lookups avoid reading all SSTable blocks, while full scans must iterate over every row in the database.

### Executor Flow (DELETE)

```
execute_delete(stmt)
  ├─ Lookup table schema from Catalog
  ├─ Require WHERE clause (DELETE without WHERE is rejected)
  │
  ├─ FAST PATH (Primary Key Lookup):
  │   Condition: WHERE clause is `primary_key = <literal>`
  │   → Direct key lookup via Engine.get()
  │   → Write Tombstone via Engine.delete()
  │   → Return "1 row deleted"
  │
  └─ SLOW PATH (Full Table Scan):
      Condition: Non-PK WHERE or range conditions
      → Full scan via Engine.iter() over all data
      → For each row matching the table's key prefix:
        → Evaluate WHERE clause via ExpressionEvaluator
        → Collect matching keys
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
  │   → Re-serialize and write back via Engine.put()
  │   → Return "1 row updated"
  │
  └─ SLOW PATH (Full Table Scan):
      Condition: Non-PK WHERE or range conditions
      → Full scan via Engine.iter() over all data
      → For each row matching the table's key prefix:
        → Evaluate WHERE clause via ExpressionEvaluator
        → Apply SET assignments to matching rows
        → Re-serialize and write back via Engine.put()
      → Return "{n} row(s) updated"
```

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
```

### Storage Key Format
```
"{table_name}:{primary_key_value}"
```
Example: `"users:42"`, `"products:abc123"`

### Row Serialization Format
```
"column1=i:42|column2=t:hello|column3=t:world"
```
- `i:` prefix for integers
- `t:` prefix for text
- `|` separator between columns
- Columns are stored in BTreeMap order (alphabetical) in the serialized format

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

- No ORDER BY or LIMIT support
- No secondary indexes
- Primary key is implicitly the first column (no explicit PK syntax)
- No transaction support
- CLI uses raw Command interface, not SQL
- MergeIterator is no longer used (replaced by UnifiedStorageIterator) — should be removed
- Parser panics on syntax errors instead of returning Result
- memtable_limit is hardcoded to 1000
- Block cache size is hardcoded to 64 blocks