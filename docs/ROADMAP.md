# ArchDB Roadmap

> **Current Phase:** Phase 5 — Testing & SQL Completeness
> **Overall Progress:** ~45% (storage engine + core SQL + advanced SQL + data integrity complete)
> **Next Milestone:** SQL regression tests, NULL support, DEFAULT values, constraints

---

## 🟢 Phase 1 — Storage Engine (Completed)

- [x] Row serialization/deserialization
- [x] Storage key generation
- [x] Block-based SSTables
- [x] Bloom filters
- [x] Block index
- [x] WAL (Write-Ahead Log) with CRC checksums
- [x] Memtable (BTreeMap-based in-memory store)
- [x] Snappy compression for SSTable blocks
- [x] CRC32 checksums for data integrity
- [x] Block Cache (LRU eviction)
- [x] Merge Iterator (k-way merge across iterators)
- [x] Unified StorageIterator (single iterator over memtable + all SSTables)
- [x] Multi-level storage (L0, L1, L2)
- [x] Size-tiered compaction (L0 → L1, L1 → L2)
- [x] Leveled compaction
- [x] Background compaction worker (async via mpsc channel)
- [x] Manifest (persistent metadata for SSTable tracking)
- [x] Manifest checkpointing
- [x] SSTable splitting during compaction (avoids oversized files)
- [x] Tombstones
- [x] Recovery

## 🟢 Phase 2 — Core SQL (Completed)

### DDL
- [x] CREATE TABLE
- [x] CREATE INDEX

### DML
- [x] INSERT
- [x] UPDATE
- [x] DELETE

### SELECT
- [x] Projection
- [x] WHERE clause
- [x] Primary-key lookup optimization
- [x] Table scan
- [x] Secondary-index lookup (exists but has a performance bug — needs fixing)
- [x] Range index lookup (exists but has a performance bug — needs fixing)

## 🟢 Phase 3 — Advanced SQL (Completed)

### Aggregates
- [x] COUNT
- [x] MIN
- [x] MAX
- [x] SUM
- [x] AVG

### GROUP BY
- [x] GROUP BY
- [x] HAVING
- [x] Multiple aggregate functions
- [x] Multiple GROUP BY columns
- [x] HAVING with aggregates
- [x] HAVING with grouped columns

### Query Modifiers
- [x] ORDER BY
- [x] LIMIT
- [x] DISTINCT

### Boolean Expressions
- [x] = (Equal)
- [x] != (NotEqual)
- [x] > (GreaterThan)
- [x] >= (GreaterThanOrEqual)
- [x] < (LessThan)
- [x] <= (LessThanOrEqual)
- [x] AND
- [x] OR
- [x] Parentheses for precedence override

## 🟢 Phase 4 — Data Integrity (Completed)

- [x] Type validation on INSERT
- [x] Type validation on UPDATE
- [x] Centralized `validate_row_types()` helper

---

## 🟡 Phase 5 — Testing & SQL Completeness (Current)

### Testing Infrastructure
- [x] SQL integration test harness (`TestDB` in `tests/common/mod.rs`)
- [ ] Regression test suite — every bug fix includes a failing test first
- [ ] Recovery tests — shutdown/restart data persistence
- [ ] Performance benchmarks

### SQL Completeness
- [ ] NULL support in storage and queries
- [ ] `IS NULL` / `IS NOT NULL` operators
- [ ] `DEFAULT` column values
- [ ] `NOT NULL` constraint enforcement
- [ ] `UNIQUE` constraint (non-primary-key)
- [ ] `CHECK` constraint

### Query Features
- [ ] OFFSET — `LIMIT 10 OFFSET 20`
- [ ] Multiple ORDER BY columns — `ORDER BY age DESC, id ASC`
- [ ] Aliases (AS) — `SELECT age AS years`, `SELECT COUNT(*) AS total`
- [ ] Arithmetic expressions — `SELECT salary * 12`, `SELECT age + 10`
- [ ] Built-in scalar functions — `UPPER(name)`, `LOWER(name)`, `LENGTH(name)`

---

## 🔵 Phase 6 — Indexes

- [ ] Fix secondary index performance bug
- [ ] Composite indexes — `CREATE INDEX ON users(age, department)`
- [ ] Covering indexes (avoid reading table rows when index contains everything)
- [ ] Range index scan optimizations

---

## 🔵 Phase 7 — Planner

- [ ] Rule-based planner improvements
- [ ] Cost-based optimizer (estimate cardinality, choose cheapest plan)
- [ ] Predicate pushdown (push WHERE clauses deeper into execution)
- [ ] Projection pushdown (only fetch needed columns)
- [ ] Join ordering

---

## � Phase 8 — JOIN Engine

- [ ] INNER JOIN
- [ ] LEFT JOIN
- [ ] RIGHT JOIN
- [ ] Subqueries — `SELECT * FROM users WHERE id IN (...)`
- [ ] EXISTS / NOT EXISTS
- [ ] UNION / INTERSECT / EXCEPT

---

## 🟣 Phase 9 — Transactions

- [ ] `BEGIN` / `COMMIT` / `ROLLBACK`
- [ ] Lock manager
- [ ] MVCC (Multi-Version Concurrency Control)
- [ ] Snapshot isolation
- [ ] Concurrent readers
- [ ] Concurrent writers

---

## � Phase 10 — Storage Optimizations

- [ ] Buffer pool
- [ ] Page cache
- [ ] Prefix compression
- [ ] Better WAL batching
- [ ] Crash recovery improvements

---

## ⚪ Phase 11 — Administration

- [ ] `SHOW TABLES`
- [ ] `SHOW INDEXES`
- [ ] `DESCRIBE table`
- [ ] `EXPLAIN query`
- [ ] `ANALYZE table`
- [ ] `ALTER TABLE` (ADD/DROP columns)
- [ ] `DROP TABLE`
- [ ] Views
- [ ] Foreign Keys
- [ ] Prepared statements

---

## ⚪ Phase 12 — Server Mode

- [ ] TCP server
- [ ] Wire protocol
- [ ] Authentication
- [ ] Multiple concurrent clients
- [ ] Connection pooling

---

## ⚪ Phase 13 — Distributed Features

- [ ] Replication
- [ ] Sharding
- [ ] Distributed transactions