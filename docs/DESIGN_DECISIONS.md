# ArchDB Design Decisions

This document records the rationale behind key architectural and implementation decisions in ArchDB. It exists to preserve context that would otherwise be lost over time.

---

## Why BTreeMap instead of HashMap for the memtable?

**Decision:** Use `BTreeMap<String, Value>` for the in-memory memtable.

**Reason:** Range scans require sorted key order. When flushing the memtable to an SSTable, data must be written in sorted order. A `BTreeMap` maintains keys in sorted order naturally, so no sorting step is needed before writing. A `HashMap` would require an explicit sort before flush, adding overhead and complexity. Additionally, the `Engine.iter()` method needs to produce a unified sorted view of all data — the memtable iterator must yield keys in order.

---

## Why Snappy compression instead of zstd or gzip?

**Decision:** Use Snappy (via the `snap` crate) for SSTable block compression.

**Reason:** In an LSM database, reads are far more frequent than writes, and read latency is critical. Snappy prioritizes decompression speed over compression ratio. It is typically 2-5x faster at decompression than zstd or gzip, while still providing reasonable space savings (~40-50%). For an embedded database where individual blocks are decompressed on each read, fast decompression is more important than maximum compression.

---

## Why Bloom filters?

**Decision:** Maintain a Bloom filter per SSTable, checked before any disk read.

**Reason:** An LSM database may have hundreds of SSTables across multiple levels. Without Bloom filters, every `get()` would need to search every SSTable (or at least read each table's index). With Bloom filters, most SSTables can be skipped with a simple memory-only check. The false positive rate is configurable (default 1%), meaning 99% of irrelevant SSTables are skipped. This is especially important at higher levels (L1, L2) where tables are large.

---

## Why block cache instead of record cache?

**Decision:** Cache decompressed blocks (groups of records) rather than individual records.

**Reason:** SSTables are read block-by-block. When a single record is requested, the entire block containing it is read from disk and decompressed. Caching at the block level means subsequent reads of nearby keys (within the same block) are served from cache without additional disk I/O or decompression. This exploits spatial locality — if a user reads one key, they are likely to read adjacent keys. A record-level cache would require more metadata and provide less benefit for sequential access patterns.

---

## Why Engine::with_storage_path() instead of always using "storage/temp"?

**Decision:** Added a `with_storage_path(path)` constructor and refactored `new()` to delegate to it.

**Reason:** Previously, all tests shared the same WAL storage path (`storage/temp`), which caused race conditions when tests ran in parallel. The WAL uses append-only files with rotation, so concurrent writes from multiple tests would corrupt the log. By giving each test its own storage path (via an `AtomicU64` counter), tests can run in parallel without interfering with each other. The `with_storage_path` constructor also makes the engine usable in non-test scenarios where the caller wants to control where data is stored.

---

## Why Engine.iter() instead of scanning each SSTable separately?

**Decision:** Provide a single `UnifiedStorageIterator` that merges all sources (memtable + all SSTables) into one ordered iteration.

**Reason:** SQL operations like `SELECT * FROM table` or `UPDATE ... WHERE ...` need to see all data in key order. Without a unified iterator, each operation would need to:
1. Collect data from the memtable
2. Read and merge data from every SSTable
3. Handle deduplication (newer versions override older ones)
4. Filter tombstones

By providing a single `UnifiedStorageIterator` that handles all of this internally, the SQL executor can simply iterate and focus on filtering and projection logic. This is the same pattern used by LevelDB and RocksDB.

---

## Why UPDATE and DELETE have fast path + slow path?

**Decision:** Implement `execute_update` and `execute_delete` with two execution strategies.

**Reason:** The most common UPDATE/DELETE pattern is modifying a single row by its primary key (e.g., `UPDATE users SET name = 'X' WHERE id = 5`). This can be handled with a direct `Engine.get()` + `Engine.put()` — O(log n) with minimal I/O. Supporting full table scans for arbitrary WHERE clauses adds flexibility without penalizing the common case.

Both operations follow the same pattern as SELECT:
- **Fast path**: When WHERE is `primary_key = <literal>`, do a direct key lookup
- **Slow path**: For all other conditions (non-PK WHERE, range conditions), do a full table scan via `Engine.iter()`, evaluate the WHERE clause with `ExpressionEvaluator`, and apply the operation to matching rows

DELETE writes a Tombstone for each matching key. UPDATE deserializes the row, applies SET assignments, re-serializes, and writes back. Both require a WHERE clause — operations without one are rejected.

---

## Why size-tiered compaction instead of level-based (RocksDB style)?

**Decision:** Use size-tiered compaction where tables within a level are compacted when the level exceeds a size threshold.

**Reason:** Simplicity of implementation. Size-tiered compaction is easier to implement and reason about than the leveled compaction used in LevelDB/RocksDB. Each level contains tables of roughly similar size, and compaction picks candidate tables based on size rather than key ranges. This avoids the complexity of maintaining level invariants (non-overlapping key ranges in higher levels) while still providing the essential benefit of reducing read amplification. The trade-off is higher write amplification and less predictable space amplification.

---

## Why background compaction via mpsc channel?

**Decision:** Trigger compaction asynchronously by sending a signal over an mpsc channel; the actual compaction runs in a background thread.

**Reason:** Compaction is I/O-intensive and can take significant time. Running it synchronously in the write path would block client operations. The design is:
1. After a flush, send a `()` signal on the channel
2. A background thread receives the signal and runs compaction
3. The SSTableManager lock is released during disk I/O, so writes can continue while compaction reads/writes files
4. The lock is only held briefly to read/write metadata

This ensures low-latency writes even during large compaction operations.

---

## Why lock-free reads during compaction?

**Decision:** Release the SSTableManager lock before performing any disk reads during `get_key()`.

**Reason:** If the lock were held during disk I/O, the background compaction worker would be blocked from making progress. Since compaction also needs to read SSTable files, holding the lock during reads creates contention. The pattern is:
1. Lock briefly → check Bloom filters → collect candidate SSTable paths and indices
2. Unlock
3. Read candidate SSTables (no lock held)
4. Background compaction can proceed concurrently

This means reads and compaction can happen in parallel, which is essential for read-heavy workloads.

---

## Why `"table_name:primary_key"` as the storage key format?

**Decision:** Use a simple string format `"{table_name}:{primary_key}"` for all storage keys.

**Reason:** Simplicity and debuggability. The format is human-readable, easy to construct, and easy to parse. The table prefix allows `scan_table()` to filter rows belonging to a specific table by checking `key.starts_with(table_name + ":")`. This is a well-known pattern (similar to how Redis namespacing works). The trade-off is slightly larger key sizes compared to binary encoding, but for an embedded database this is acceptable.

---

## Why panic in the parser instead of returning Result?

**Decision:** The SQL parser uses `panic!()` for syntax errors rather than returning `Result<Statement, Error>`.

**Reason:** At the time the parser was implemented, error handling was not yet fully designed. The parser was built for a SQL subset where inputs are expected to be well-formed. This is acknowledged technical debt — a production parser should return proper errors. For now, callers must use `std::panic::catch_unwind` if they need to handle parse failures gracefully.

---

## Why first column is implicitly the primary key?

**Decision:** In `execute_create_table`, the first column is automatically marked as the primary key.

**Reason:** The parser does not yet support `PRIMARY KEY` syntax in `CREATE TABLE`. Making the first column the PK by default is a pragmatic choice that allows INSERT/SELECT/UPDATE/DELETE to work immediately. This is documented in the roadmap as "Explicit PRIMARY KEY syntax" for Phase 3.

---

## Why `QueryResult` instead of returning `Vec<Vec<String>>` directly?

**Decision:** Wrap query results in an enum with `None`, `Message(String)`, and `Rows(Vec<Vec<String>>)` variants.

**Reason:** Different SQL statements produce different kinds of results:
- `CREATE TABLE` → success/failure message
- `INSERT` → success/failure message
- `SELECT` → rows of data (or empty set)
- `UPDATE`/`DELETE` → affected row count

Using a single enum type allows the executor to return the appropriate result type for each statement while keeping a uniform return type from `execute()`.

---

## Why is MergeIterator no longer used but still present?

**Decision:** `MergeIterator` was the original k-way merge implementation. It was replaced by `UnifiedStorageIterator` but not removed.

**Reason:** During refactoring to support SQL table scans, a new `UnifiedStorageIterator` was built that wraps the `StorageIterator` trait. The old `MergeIterator` became unused but was left in the codebase because removing it was not a priority. It should be removed in a cleanup pass.

---

## Why row serialization uses a custom text format instead of protobuf/flatbuffers?

**Decision:** Use a simple text format: `"column1=i:42|column2=t:hello"`.

**Reason:** At this stage of the project, a custom format avoids external dependencies (protobuf, flatbuffers, cap'n proto) and keeps the code self-contained. The format is easy to debug — you can read serialized rows directly. When the database needs to support schema evolution, migration, or cross-language compatibility, a structured serialization format should be adopted. For now, the simplicity is worth the trade-off.

---

## Why does `SELECT *` return columns in CREATE TABLE order instead of alphabetical order?

**Decision:** When projecting rows with `SelectItem::Wildcard`, iterate over the schema's column list rather than the row's internal BTreeMap.

**Reason:** The `Row` type stores values in a `BTreeMap<String, RowValue>`, which iterates in alphabetical key order. If `SELECT *` used BTreeMap order, columns would appear in alphabetical order (e.g., `age` before `id`), which is confusing to users who defined columns in a specific order in `CREATE TABLE`. By iterating over the schema's column list (which preserves CREATE TABLE order), the output matches user expectations. This also matches the behavior of most SQL databases, where `SELECT *` returns columns in table definition order.