# ArchDB Implementation Log

## Milestone 1: Storage Engine Complete

### 2026-06-01 — Initial Storage Engine
- Basic LSM-tree structure with memtable and SSTable
- Simple SSTable read/write without compression
- CLI interface with raw commands (Set, Get, Del)
- WAL with basic append and recovery

### 2026-06-05 — SSTable Improvements
- Added Snappy compression for SSTable blocks
- CRC32 checksums for data integrity
- Block-based storage format with index
- Binary search within blocks

### 2026-06-08 — Bloom Filters
- Added bloom filter support
- Bloom filter stored in SSTable footer
- Used for fast key existence check before disk read
- Configurable false positive rate

### 2026-06-10 — Multi-Level Storage
- Introduced L0, L1, L2 levels
- SSTable lifecycle management
- Level-based organization for efficient compaction

## Milestone 2: LSM Engine Complete

### 2026-06-12 — Size-Tiered Compaction
- Implemented size-tiered compaction strategy
- L0 → L1 compaction when L0 exceeds threshold
- L1 → L2 compaction when L1 exceeds threshold
- Overlap detection for correct compaction
- Tombstone preservation during compaction

### 2026-06-14 — Compaction Improvements
- Background compaction worker (async via mpsc channel)
- Lock-free during disk I/O (release SSTableManager lock)
- CompactionPicker for candidate selection
- SSTable splitting during compaction to avoid oversized files

### 2026-06-16 — Block Cache
- LRU block cache implementation
- CacheKey based on path + block offset
- Cache hit avoids disk read entirely
- 64-block default capacity

### 2026-06-18 — WAL Enhancements
- WAL segment rotation
- CRC checksum validation on recovery
- Corruption detection and recovery
- SyncPolicy (Always, Every N seconds)
- Checkpoint support

### 2026-06-20 — Merge Iterator
- K-way merge across multiple StorageIterators
- Deduplication (newer version wins for same key)
- Tombstone filtering option
- Used for merging memtable + SSTable iterators

### 2026-06-22 — Manifest
- Persistent SSTable metadata tracking
- AddTable/RemoveTable record types
- Checkpoint mechanism
- Log truncation after checkpoint
- Recovery on startup

## Milestone 3: SQL Layer Started

### 2026-06-25 — SQL Layer: Lexer & Parser
- Added SQL module to project
- Token types: Select, Insert, Create, Delete, Update, etc.
- Lexer with keyword recognition and string/number literals
- Parser supporting:
  - CREATE TABLE
  - INSERT
  - SELECT (with WHERE)
  - DELETE (with WHERE)
  - UPDATE (with SET/WHERE)
- Expression parsing for WHERE clauses
- AST node types

### 2026-06-27 — SQL Layer: Executor
- SQL statement executor
- CREATE TABLE implementation with catalog
- INSERT with column validation
- SELECT with primary key lookup
- UPDATE with primary key lookup
- DELETE with primary key lookup
- Row serialization/deserialization format

## Milestone 4: Full Table Scan Support

### 2026-07-01 — Enhanced SELECT Implementation
- Refactored `execute_select` with fast/slow paths:
  - Fast path: primary key equality → direct Engine.get()
  - Slow path: full table scan via Engine.iter()
- Added `can_use_primary_key_lookup()` helper
- ExpressionEvaluator for WHERE clause filtering
- Support for !=, >, >=, <, <= comparison operators

### 2026-07-02 — Compilation & Test Fixes
- Fixed DatabaseError From implementations (added From<&str>, From<String>)
- Fixed HeapItem name collision (engine.rs vs merge_iterator.rs)
- Fixed UnifiedStorageIterator (StorageRecord → BlockRecord, added peek())
- Fixed executor_tests.rs string literal issues
- Fixed engine.rs iter() method for loops
- Fixed StorageIterator trait imports in executor.rs and tests
- Fixed parser.rs expression return type (Result<Expr> → Expr)

### 2026-07-03 — Test Corrections
- Restored original test names and behaviors in executor_tests.rs
- Updated test assertions to match new execute_select semantics
- Fixed lexer_tests.rs Parser::new() calls (Lexer not &str)

### 2026-07-05 — Project Documentation
- Created ROADMAP.md with completed/remaining features
- Created ARCHITECTURE.md with full project structure and design
- Created IMPLEMENTATION_LOG.md with chronological development history
- Created DESIGN_DECISIONS.md with rationale for key design choices

### 2026-07-06 — Full Table Scans for DELETE & UPDATE
- Implemented `execute_delete` with fast/slow paths:
  - Fast path: primary key lookup → direct Engine.delete() 
  - Slow path: full table scan via Engine.iter() + ExpressionEvaluator
- Implemented `execute_update` with fast/slow paths:
  - Fast path: primary key lookup → Engine.get() + apply + Engine.put()
  - Slow path: full table scan via Engine.iter() + ExpressionEvaluator
- Both DELETE and UPDATE require a WHERE clause (rejected without one)
- UPDATE disallows modifying the primary key column
- Full table scan path scans all data (memtable + SSTables) and applies WHERE filtering
- Added helper methods `scan_table()` and `matching_rows()` shared across SELECT/DELETE/UPDATE

### 2026-07-06 — Extended Test Coverage
- Added tests for DELETE with primary key lookup
- Added tests for DELETE with non-primary key WHERE (full scan)
- Added tests for DELETE multiple matching rows
- Added tests for DELETE across memtable and SSTable boundaries
- Added tests for DELETE after flush
- Added tests for DELETE with all comparison operators (>, >=, <, <=, !=)
- Added tests for DELETE with no matching rows
- Added tests for DELETE from missing table
- Added tests for DELETE without WHERE clause
- Added tests for UPDATE with existing/non-existing rows
- Added tests for UPDATE multiple columns
- Added tests for UPDATE primary key rejection
- Added tests for SELECT with non-primary-key WHERE (using full table scan)
- Added tests for SELECT all rows across memtable + SSTable
- Fixed SELECT column ordering: `SELECT *` now returns columns in CREATE TABLE order instead of BTreeMap alphabetical order
- Fixed project_row() to accept schema column list for correct wildcard projection

### 2026-07-08 — ORDER BY & LIMIT Implementation
- Added `OrderBy` struct and `OrderDirection` enum (`Asc`/`Desc`) to AST
- Added `order_by: Option<OrderBy>` and `limit: Option<usize>` fields to `Select` struct
- Extended SQL parser with `ORDER BY column [ASC|DESC]` and `LIMIT n` clauses
- Added `Order` and `Desc` token types to lexer
- Implemented `execute_select` ORDER BY logic:
  - Sorts result rows by the specified column using the row's internal values
  - Supports ascending (default) and descending order
- Implemented `execute_select` LIMIT logic:
  - Truncates result rows to the specified limit after filtering and sorting
- ORDER BY and LIMIT can be combined (LIMIT applied after ORDER BY)

### 2026-07-09 — Test Infrastructure & Compilation Fixes
- Added `Engine::with_storage_path(path)` constructor for custom storage directories
- Refactored `Engine::new()` to delegate to `with_storage_path("storage/temp")`
- Made all tests use unique per-test WAL paths via `make_engine()` helper with `AtomicU64` counter:
  - `executor_tests.rs` — eliminated WAL path collisions between parallel tests
  - `engine_tests.rs` — replaced shared `clean_all_state()` with unique paths
  - `engine_iterator_tests.rs` — same pattern applied
- Fixed compilation errors in `executor_tests.rs`:
  - Moved `limit`/`order_by` fields to correct top-level position in `Select` struct initializers
  - Removed invalid `limit`/`order_by` fields from `Delete` and `Update` struct initializers
- Fixed `test_delete_multiple_rows` column ordering assertion (alphabetical from BTreeMap)

## Known Issues & Technical Debt
- `MergeIterator` is no longer used directly (replaced by `UnifiedStorageIterator`) — should be removed
- `engine_iterator.rs` wraps `UnifiedStorageIterator` with minimal logic — could be inlined
- `parser.rs` and `command.rs` both exist for parsing input — legacy CLI parser vs SQL parser
- Some tests rely on `panic!` behavior rather than `Result` return types
- No proper error propagation in parser (uses panic! for invalid syntax)
- memtable_limit is hardcoded to 1000 in Engine::new()
- Block cache size is hardcoded to 64 blocks