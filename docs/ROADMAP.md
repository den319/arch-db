# ArchDB Roadmap

## Storage Engine
- [x] WAL (Write-Ahead Log) with CRC checksums
- [x] Memtable (BTreeMap-based in-memory store)
- [x] SSTables (Sorted String Tables) with block-based storage
- [x] Bloom Filter (probabilistic key existence check)
- [x] Snappy compression for SSTable blocks
- [x] CRC32 checksums for data integrity
- [x] Block Cache (LRU eviction)
- [x] Merge Iterator (k-way merge across iterators)
- [x] Unified StorageIterator (single iterator over memtable + all SSTables)
- [x] Multi-level storage (L0, L1, L2)
- [x] Size-tiered compaction (L0 → L1, L1 → L2)
- [x] Background compaction worker (async via mpsc channel)
- [x] Manifest (persistent metadata for SSTable tracking)
- [x] SSTable splitting during compaction (avoids oversized files)

## SQL Layer
- [x] Lexer (tokenizer for SQL statements)
- [x] Parser (SQL → AST)
- [x] CREATE TABLE
- [x] INSERT
- [x] SELECT (primary key lookup + full table scan)
- [x] UPDATE (primary key lookup + full table scan)
- [x] DELETE (primary key lookup + full table scan)
- [x] Comparison operators (=, !=, >, >=, <, <=)
- [x] Expression evaluator (WHERE clause filtering)
- [x] Catalog (table schema metadata)
- [x] Row serialization/deserialization

## Remaining Work (By Phase)

### Phase 1 — Core SQL Operations
- [x] UPDATE with full table scan — scan all rows, evaluate WHERE, update matching rows
- [x] DELETE with full table scan — scan all rows, evaluate WHERE, delete matching rows
- [ ] SQL CLI — replace raw Command interface with SQL-based REPL

### Phase 2 — Query Features
- [x] ORDER BY — sort result sets (ASC/DESC per column)
- [x] LIMIT — restrict number of returned rows

### Phase 3 — Indexing
- [x] Explicit PRIMARY KEY syntax in CREATE TABLE — with `PRIMARY KEY` keyword support
- [x] Secondary indexes — CREATE INDEX support with index maintenance on INSERT/UPDATE/DELETE
- [x] Index metadata persistence and recovery across restarts

### Phase 4 — Query Optimization
- [ ] Use indexes for query execution — index-scan lookups for WHERE clauses on indexed columns
- [ ] Query planner — choose index scan vs. table scan based on cost estimation

### Phase 5 — Transactions & Concurrency
- [ ] Transactions — atomic multi-statement operations
- [ ] MVCC (Multi-Version Concurrency Control)
- [ ] Snapshot Isolation

## Future Ideas
- JOIN (INNER, LEFT, RIGHT)
- GROUP BY and HAVING
- Aggregate functions (COUNT, SUM, AVG, MIN, MAX)
- ALTER TABLE (ADD/DROP columns)
- DROP TABLE
- Composite indexes (multi-column)
- B+Tree indexes for range queries
- Cost-based optimizer
- Replication
- Prepared statements
- Connection pooling
- Persist `primary_key` flag in catalog schema serialization