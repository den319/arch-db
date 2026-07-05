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
- [ ] MVCC (Multi-Version Concurrency Control)
- [ ] Snapshot Isolation

## SQL Layer
- [x] Lexer (tokenizer for SQL statements)
- [x] Parser (SQL → AST)
- [x] CREATE TABLE
- [x] INSERT
- [x] SELECT (primary key lookup + full table scan)
- [x] UPDATE (primary key lookup only)
- [x] DELETE (primary key lookup only)
- [x] Comparison operators (=, !=, >, >=, <, <=)
- [x] Expression evaluator (WHERE clause filtering)
- [x] Catalog (table schema metadata)
- [x] Row serialization/deserialization

## Remaining Work (Priority Order)
1. **UPDATE with full table scan** — scan all rows, evaluate WHERE, update matching rows
2. **DELETE with full table scan** — scan all rows, evaluate WHERE, delete matching rows
3. **SQL CLI** — replace raw Command interface with SQL-based REPL
4. **ORDER BY** — sort result sets
5. **LIMIT** — restrict number of returned rows
6. **Secondary indexes** — non-primary key index support
7. **Primary key declaration in CREATE TABLE** — explicit PK syntax instead of first-column default
8. **Query planner** — choose index scan vs. table scan based on cost estimation
9. **Transactions** — atomic multi-statement operations