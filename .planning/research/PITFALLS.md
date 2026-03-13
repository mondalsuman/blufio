# Domain Pitfalls: sqlite-vec Migration, Performance Benchmarking, and Injection Defense Hardening

**Domain:** Adding vector search extension, benchmarking suite, and security hardening to existing Rust AI agent platform
**Researched:** 2026-03-13
**Milestone:** v1.6 Performance & Scalability Validation
**Overall confidence:** HIGH (grounded in actual codebase analysis + official documentation)

---

## Critical Pitfalls

Mistakes that cause data loss, broken production deployments, or require rewrites.

---

### Pitfall 1: SQLCipher + sqlite-vec Extension Initialization Order Conflict

**What goes wrong:** The project uses `rusqlite` 0.37 with the `bundled-sqlcipher-vendored-openssl` feature. SQLCipher requires `PRAGMA key` to be the **first statement** on every new connection before any data access. The sqlite-vec crate registers itself via `sqlite3_auto_extension()`, which triggers initialization code when a connection opens. If the auto-extension attempts to read or create virtual table schema before `PRAGMA key` is applied, the connection fails with "file is encrypted or is not a database."

**Why it happens:** `sqlite3_auto_extension()` is a global process-wide hook. Extensions registered this way run on every `sqlite3_open()` call. The current `open_connection()` in `database.rs` (line 99-143) applies `PRAGMA key` after `tokio_rusqlite::Connection::open()` returns -- but the auto-extension may have already triggered during the open call itself. The vec0 virtual table's `xConnect`/`xCreate` methods will try to access `sqlite_master` to check for existing tables, which fails on encrypted databases without the key.

**Consequences:** Every database connection fails. Complete system outage on startup. Impossible to recover without code changes.

**Prevention:**
- Do NOT use `sqlite3_auto_extension()` for sqlite-vec registration. Instead, register the extension per-connection **after** `PRAGMA key` has been applied and verified.
- Use `rusqlite::Connection::handle()` to get the raw `sqlite3*` pointer, then call `sqlite3_vec_init()` manually within the `conn.call()` closure, after the encryption key PRAGMA.
- The registration sequence must be: `open() -> PRAGMA key -> verify key -> sqlite3_vec_init() -> WAL mode -> other PRAGMAs -> migrations`.
- Modify `open_connection()` in `blufio-storage/src/database.rs` to accept an optional extension initializer closure.

**Detection:** Integration test with `BLUFIO_DB_KEY` set that creates a vec0 table and runs a KNN query. This test must pass on both fresh databases and existing encrypted databases.

**Confidence:** HIGH -- based on SQLCipher documentation ("key must be set before the first operation") and sqlite-vec's use of `sqlite3_auto_extension` pattern.

---

### Pitfall 2: rusqlite bundled-sqlcipher vs sqlite-vec's Bundled SQLite Conflict

**What goes wrong:** The `sqlite-vec` crate (v0.1.6) statically compiles its own copy of the sqlite-vec C source using the `cc` crate, but expects to link against the **same SQLite library** that `rusqlite` uses. However, `rusqlite` with `bundled-sqlcipher-vendored-openssl` compiles **SQLCipher** (a fork of SQLite), not standard SQLite. The `sqlite-vec` crate's `sqlite3_vec_init` function signature must match the SQLCipher fork's internal ABI. If there's an ABI mismatch or if sqlite-vec tries to use symbols not present in SQLCipher, you get link errors or undefined behavior at runtime.

**Why it happens:** The `sqlite-vec` Rust crate compiles C source against sqlite3.h headers. If those headers come from upstream SQLite rather than SQLCipher, the compiled extension may reference different internal structures. SQLCipher adds encryption-related fields to connection structures and changes some internal function signatures.

**Consequences:** Link-time errors (best case), runtime segfault (worst case), or subtle corruption where the extension appears to work but produces incorrect results due to struct layout differences.

**Prevention:**
- Do NOT add `sqlite-vec` crate as a cargo dependency directly. Instead, vendor the sqlite-vec C source files (`sqlite-vec.c`, `sqlite-vec.h`) directly into `blufio-storage` and compile them with `cc` using the same SQLCipher headers that `libsqlite3-sys` provides.
- Set the `cc::Build` include path to point at the SQLCipher headers from `libsqlite3-sys`'s `DEP_SQLITE3_INCLUDE_DIR` environment variable (emitted by libsqlite3-sys's build script).
- Alternatively, investigate if sqlite-vec can be compiled with just a header include and the init function exposed, linking against the same `libsqlite3-sys` that rusqlite uses.
- Write a smoke test that creates a vec0 table, inserts vectors, and runs a KNN query with SQLCipher encryption enabled.

**Detection:** Compile-time link errors catch the obvious case. Runtime test with encryption enabled catches the subtle case. A CI job that runs `cargo test` with `BLUFIO_DB_KEY=test-key` is essential.

**Confidence:** HIGH -- this is a well-known pattern in the SQLite extension ecosystem; the sqlite-vec documentation's Rust example assumes `rusqlite` with standard `bundled` feature, not SQLCipher.

---

### Pitfall 3: In-Memory Full-Table Scan Removal Breaks Scoring Pipeline

**What goes wrong:** The current `vector_search()` method in `retriever.rs` (line 190-218) loads ALL active embeddings into memory via `store.get_active_embeddings()`, computes cosine similarity against each one in Rust, and filters by threshold. Migrating to sqlite-vec's vec0 KNN query changes the return semantics: vec0 returns the K nearest neighbors with distances, not similarity scores above a threshold. sqlite-vec's `vec_distance_cosine()` returns **distance** (1 - similarity), not similarity.

**Why it happens:** It is natural to assume a KNN search returns similar data to the existing search. But sqlite-vec returns distance metrics (lower is better) while the existing pipeline expects similarity scores (higher is better). Additionally, vec0 returns exactly K results regardless of quality, while the current implementation returns only results above `similarity_threshold` (which may be fewer than K).

**Consequences:** Reversed ranking (worst matches ranked first), broken RRF fusion scores, and memories with very low relevance injected into context (wasting tokens and degrading agent quality).

**Prevention:**
- Create an explicit conversion layer: `similarity = 1.0 - cosine_distance` applied immediately after vec0 query results.
- Maintain the similarity threshold filter AFTER the conversion: discard results where `similarity < config.similarity_threshold`.
- The vec0 query should request `k = max_retrieval_results * 2` (overfetch) to account for post-filtering.
- Add a regression test comparing the top-5 results from the new sqlite-vec path against the old in-memory path for a fixed set of 100 test embeddings. Results must match within floating-point tolerance.

**Detection:** Unit test with known embeddings where expected similarity scores are precomputed. Any ranking reversal is immediately visible.

**Confidence:** HIGH -- confirmed from sqlite-vec API reference: `vec_distance_cosine()` returns distance (0 = identical, 2 = opposite), not similarity.

---

### Pitfall 4: vec0 Virtual Table Cannot Filter on External Table Columns

**What goes wrong:** The current memory system filters on `status = 'active'` AND `classification != 'restricted'` AND `deleted_at IS NULL` (see `store.rs` lines 153-168). These columns live in the `memories` table. When using vec0 for KNN search, the query planner executes the KNN search FIRST, then applies JOIN/WHERE filters from the memories table AFTER. This means the K nearest results may include inactive, restricted, or deleted memories, and after filtering those out, you get fewer than K results (possibly zero).

**Why it happens:** SQLite's query planner for virtual tables executes the virtual table's `xBestIndex`/`xFilter` before applying external WHERE clauses from JOINed tables. This is a fundamental limitation documented in sqlite-vec issue #196.

**Consequences:** Returning deleted/restricted memories in search results (security violation for GDPR). Returning fewer results than expected. Inconsistent behavior between vector search and BM25 search (which does filter correctly via SQL WHERE).

**Prevention:**
- Add `status` as a **metadata column** (TEXT type) on the vec0 virtual table. This allows `WHERE status = 'active'` to be applied during the KNN search itself.
- Add `classification` as a metadata column similarly.
- For `deleted_at IS NULL`, use a metadata column `is_deleted INTEGER DEFAULT 0` (since vec0 metadata columns do not support NULL comparisons).
- The vec0 schema should look like:
  ```sql
  CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec USING vec0(
      memory_id TEXT PRIMARY KEY,
      embedding float[384],
      status TEXT,
      classification TEXT,
      is_deleted INTEGER
  );
  ```
- Maintain the metadata columns in sync with the memories table via application logic (UPDATE vec0 row when status changes in memories table). Triggers from the memories table cannot update vec0 virtual tables.

**Detection:** Test that searches with 100 memories (50 active, 30 superseded, 20 deleted) only returns active non-restricted results. Compare result count against the old path.

**Confidence:** HIGH -- confirmed by sqlite-vec documentation: metadata columns CAN appear in WHERE for KNN queries, but columns from JOINed tables CANNOT.

---

### Pitfall 5: Migration Data Loss on vec0 Table Population

**What goes wrong:** Existing embeddings are stored as BLOBs in the `memories` table using `vec_to_blob()` which stores little-endian f32 bytes. When creating and populating the vec0 virtual table, the BLOB format must match exactly what sqlite-vec expects. If there is a byte-order mismatch, padding difference, or alignment issue, the vec0 table silently stores corrupted vectors, and all KNN queries return garbage results.

**Why it happens:** The current `vec_to_blob()` (types.rs line 139) uses `f32::to_le_bytes()` and concatenates them. sqlite-vec's `vec_f32()` function accepts BLOBs where each float is stored as IEEE 754 little-endian -- which is the same format. However, if anyone passes the BLOB through `vec_f32()` constructor during INSERT but the BLOB is already in the correct format, double-conversion could occur. Also, the BLOB length must be exactly `dimensions * 4` bytes or sqlite-vec rejects it.

**Consequences:** Silent data corruption. KNN searches return incorrect results. No error is raised because the vectors are valid floats, just wrong ones.

**Prevention:**
- Verify BLOB format compatibility with an explicit test: insert a known 384-dim embedding via the existing `vec_to_blob()`, read it back via `vec_f32()` then `vec_to_json()`, and confirm values match.
- The migration INSERT should use raw BLOB insertion into vec0, NOT wrapping in `vec_f32()`: `INSERT INTO memories_vec(memory_id, embedding, status, classification, is_deleted) SELECT id, embedding, status, classification, CASE WHEN deleted_at IS NULL THEN 0 ELSE 1 END FROM memories WHERE embedding IS NOT NULL AND length(embedding) = 1536` (384 * 4 = 1536 bytes).
- Add a post-migration validation step: pick 10 random memories, compute cosine similarity in Rust (old path) and via vec0 KNN (new path), verify results match within tolerance.
- Run migration in a transaction. If validation fails, ROLLBACK.

**Detection:** Post-migration validation query comparing old and new paths for known test vectors.

**Confidence:** HIGH -- the existing BLOB format (little-endian f32 concatenation) matches sqlite-vec's expected format, but verification is non-negotiable.

---

## Moderate Pitfalls

Mistakes that cause performance issues, test flakiness, or significant rework.

---

### Pitfall 6: vec0 Table Size Doubles Memory Footprint

**What goes wrong:** The current system stores embeddings as BLOBs in the `memories` table. Adding a vec0 virtual table means the same embedding data is stored TWICE: once in the memories table (for backward compatibility and direct access) and once in vec0's internal storage (for KNN indexing). For 10K entries at 384 dimensions, that is `10000 * 384 * 4 = 15.36 MB` duplicated. At the 50-80MB idle memory target, this 15MB overhead is significant (roughly 20-30% of budget).

**Why it happens:** vec0 is a separate virtual table with its own storage. It does not reference the original BLOB -- it copies the data.

**Prevention:**
- After migration is validated and stable, consider dropping the `embedding` BLOB column from the `memories` table entirely. The vec0 table becomes the sole source of vector data.
- If backward compatibility requires keeping the BLOB (e.g., for the in-memory cosine similarity fallback path during transition), plan a deprecation timeline.
- Monitor database file size and RSS before and after migration. Add a benchmark assertion: `db_size_after <= db_size_before * 1.5`.
- Use int8 quantization (`vec_quantize_i8()`) if precision allows: 384 * 1 byte = 384 bytes vs 384 * 4 = 1536 bytes per vector. This trades precision for a 4x size reduction. Test that quantized vectors produce acceptable recall (>95% overlap with float32 top-10 results).

**Detection:** Database file size check in CI. Memory RSS measurement before/after migration.

**Confidence:** MEDIUM -- the duplication is certain; the impact depends on whether the embedding column is retained.

---

### Pitfall 7: FTS5 Sync Triggers Do Not Work with vec0

**What goes wrong:** The current system uses SQLite triggers (`memories_ai`, `memories_ad`, `memories_au`) to keep the FTS5 `memories_fts` table in sync with the `memories` table. Developers assume the same pattern will work for vec0. It does NOT. SQLite triggers cannot INSERT into or UPDATE virtual tables (including vec0). Any attempt to create a trigger that writes to a vec0 table fails silently or errors.

**Why it happens:** Virtual tables in SQLite have limited trigger support. The `xUpdate` method of virtual tables is not invoked by trigger-based INSERTs in all cases, and vec0 specifically does not support being the target of trigger actions.

**Consequences:** The vec0 table becomes stale after any memory INSERT, UPDATE, or DELETE. New memories are not findable via vector search. Deleted memories continue to appear in results.

**Prevention:**
- ALL vec0 writes must happen in application code (Rust), not SQL triggers.
- Modify `MemoryStore::save()` to also INSERT into `memories_vec`.
- Modify `MemoryStore::soft_delete()` to UPDATE the `is_deleted` metadata column in `memories_vec`.
- Modify `MemoryStore::supersede()` to UPDATE the `status` metadata column in `memories_vec`.
- Modify `MemoryStore::batch_evict()` to DELETE from `memories_vec` in the same transaction.
- Wrap all paired writes (memories table + vec0 table) in a single transaction to maintain consistency.
- Add a consistency check command (`blufio doctor`) that verifies row counts match between `memories` and `memories_vec`.

**Detection:** Integration test that inserts a memory, verifies it appears in vec0 KNN search, then deletes it and verifies it no longer appears.

**Confidence:** HIGH -- confirmed from SQLite documentation on virtual table limitations.

---

### Pitfall 8: Benchmark Suite Measures Artifacts Instead of Production Behavior

**What goes wrong:** Criterion benchmarks run in isolation with synthetic data, optimized compiler settings, and no concurrent load. Binary size is measured from `target/release/blufio` which includes debug symbols unless `strip = true` is set. RSS measurements from `jemalloc_ctl::stats` only show allocator-tracked memory, missing mmap'd files (ONNX model, SQLite database pages). The benchmarks pass but production performance differs by 2-5x.

**Why it happens:** Benchmarks naturally optimize for measurability over realism. Common mistakes:
1. Using `criterion::black_box` on data that would normally be optimized away, but not on the control flow that would.
2. Measuring hot-cache performance when production is cold-cache.
3. Using in-memory SQLite (`:memory:`) instead of WAL-mode file-backed SQLite with encryption.
4. Omitting the ONNX model load in memory benchmarks (the model is around 23MB resident).

**Consequences:** False confidence in performance. Binary size claims that do not match actual distribution. Memory limits exceeded in production.

**Prevention:**
- **Binary size:** Strip symbols in release profile (`[profile.release] strip = true`). Measure after strip. Measure with `wc -c target/release/blufio`, not `ls -la` (which may show different units).
- **Memory RSS:** Use `/proc/self/statm` (Linux) or `mach_task_info` (macOS) for total RSS including mmap. Or use `tikv-jemalloc-ctl` with `epoch::advance()` before reading `stats::resident::read()` for allocator-tracked RSS.
- **Benchmark realism:** Create benchmarks that use file-backed encrypted SQLite, not `:memory:`. Include ONNX model initialization in the setup phase, measure the steady-state.
- **Binary size budget:** Add a CI assertion: `size target/release/blufio | awk '{print $1}'` must be <= 50MB (50 * 1024 * 1024 bytes).
- **The existing `bench_results` table** (V11 migration) already tracks `peak_rss_bytes` and `system_info`. Use it. Store baselines and fail CI if RSS exceeds baseline by >10%.

**Detection:** CI jobs that measure and compare against stored baselines. Manual verification against VPS deployment.

**Confidence:** HIGH -- these are well-known benchmarking pitfalls; the existing bench infrastructure partially addresses them but needs extension.

---

### Pitfall 9: Injection Pattern Expansion Causes False Positive Epidemic

**What goes wrong:** Adding more regex patterns to the classifier increases recall (catches more attacks) but also increases false positives. The current 11 patterns (patterns.rs) are conservative. Expanding to cover Unicode obfuscation, synonym substitution ("discard prior directives"), base64 encoding, and zero-width character injection can easily push false positive rates above 5%, causing legitimate user messages to be flagged or blocked.

**Why it happens:** Regex-based detection operates on byte patterns, not semantics. Broadening patterns to catch evasion attempts (e.g., `(?i)discard\s+(all\s+)?(prior|preceding)\s+` catches "discard prior directives" but also "discard all prior reservations"). The L1 classifier uses severity-weighted scoring (0.1-0.5 per pattern), and with more patterns matching on benign text, cumulative scores cross the blocking threshold (default 0.95 for users).

**Consequences:** Users get messages blocked. Support burden increases. Users lose trust in the system. Operators disable injection defense entirely to stop false positives, removing real protection.

**Prevention:**
- Maintain a false-positive test corpus alongside the attack test corpus. Every new pattern must pass BOTH: catches at least one known attack AND does not flag any message in the benign corpus.
- The benign corpus should include at least 100 realistic user messages covering: scheduling ("send this to the team"), knowledge base queries ("forget about the old API, tell me about the new one"), system administration ("you are now running version 3.1"), and developer discussions ("[INST] is a token format used by Llama").
- Use specificity over breadth: prefer `(?i)^\s*ignore\s+all\s+previous\s+instructions` (anchored, full phrase) over `(?i)ignore.*instructions` (matches "ignore the cooking instructions").
- Track false positive rate as a Prometheus metric: `blufio_injection_false_positive_rate` (requires manual labeling of a sample set).
- Keep the `dry_run` mode as the default for newly added patterns. Only promote to `log` mode after observing zero false positives over a validation period.

**Detection:** Automated test suite with benign corpus. Manual review of `dry_run` logs before promotion.

**Confidence:** HIGH -- the OWASP Prompt Injection Prevention Cheat Sheet explicitly warns that "Tier 0 regex follows the antivirus signature path, with 191 patterns achieving only 23% recall and diminishing returns."

---

### Pitfall 10: tokio-rusqlite Single-Writer Bottleneck During Migration

**What goes wrong:** The vec0 table population migration must process all existing memories (potentially 10K+ rows, each with 1536-byte embeddings). The current architecture uses a single `tokio_rusqlite::Connection` for all operations. The migration runs inside `conn.call()`, blocking the single writer thread. During this time, ALL other database operations (session saves, message queues, cost ledger writes, audit trail) are blocked. On a VPS with slow I/O, migrating 10K entries could take 5-30 seconds.

**Why it happens:** `tokio-rusqlite` wraps `rusqlite::Connection` in a single dedicated thread. All `conn.call()` invocations queue behind each other. A long-running migration starves all other database consumers.

**Consequences:** Agent stops responding during migration. Health checks fail. systemd watchdog may kill the process. Messages are dropped from the queue.

**Prevention:**
- Run the migration in batches of 500 rows, yielding between batches: `INSERT INTO memories_vec ... SELECT ... FROM memories LIMIT 500 OFFSET N`. Between batches, release the `conn.call()` closure so other operations can proceed.
- Set a generous timeout for the migration call. The current `busy_timeout = 5000ms` may not be enough for a large single-transaction migration.
- Log progress: "Migrated 500/10000 memories to vec0 (5.0%)".
- Consider running the migration as a CLI command (`blufio migrate-vectors`) rather than on startup, so operators can control timing.
- If running at startup, add a migration status flag in the database so it is idempotent: `INSERT OR IGNORE INTO migration_flags (name, completed_at) VALUES ('v15_vec0_migration', ...)`.

**Detection:** Test with 5000 synthetic memories to validate migration time. Monitor systemd watchdog during migration.

**Confidence:** HIGH -- the single-writer pattern is documented in `database.rs` line 189: "all reads and writes go through the single background thread."

---

### Pitfall 11: Benchmark Regression CI Flaky on Shared CI Runners

**What goes wrong:** Criterion benchmarks use wall-clock time and statistical tests to detect regressions. On shared CI runners (GitHub Actions, etc.), other workloads cause variance that exceeds Criterion's default 5% noise threshold. Benchmarks that ran fine locally become flaky in CI, either producing false regression alerts or (worse) missing real regressions because the noise floor is too high.

**Why it happens:** Criterion's statistical methodology assumes a stable measurement environment. Shared cloud VMs have variable CPU frequency, memory bandwidth, and I/O latency due to noisy neighbors.

**Consequences:** CI pipeline has permanent flaky tests. Developers disable or ignore benchmark assertions. Real regressions slip through.

**Prevention:**
- Use `--significance-level 0.01` instead of default 0.05 to reduce false positives.
- Use `Criterion::measurement_time(Duration::from_secs(10))` to increase sample size.
- For CI regression detection, use percentage thresholds rather than statistical tests: `assert!(current_median < baseline_median * 1.20)` (20% regression budget accounts for CI noise).
- Store benchmark baselines in the `bench_results` SQLite table and compare against them in a post-benchmark script, NOT in Criterion's built-in comparison.
- For memory and binary size benchmarks, these are deterministic -- no CI noise. Use exact thresholds for these.
- Consider running timing benchmarks only on self-hosted runners or dedicated CI machines.

**Detection:** Track CI benchmark variance over 20 runs. If stddev > 15% of median, the benchmark is too noisy for CI.

**Confidence:** MEDIUM -- standard CI practice; the existing `bench_results` table supports this approach.

---

## Minor Pitfalls

Mistakes that cause confusion, tech debt, or minor rework.

---

### Pitfall 12: vec0 Primary Key Must Be Declared Explicitly

**What goes wrong:** The existing memories table uses `id TEXT PRIMARY KEY`. When creating the vec0 table, developers omit the primary key declaration, expecting vec0 to auto-generate integer rowids. But then there is no way to JOIN vec0 results back to the memories table without the original memory ID.

**Prevention:** Always declare `memory_id TEXT PRIMARY KEY` as the first column of the vec0 table. Use the same ID format as the memories table.

**Confidence:** HIGH -- confirmed in sqlite-vec documentation.

---

### Pitfall 13: HMAC Boundary Token Patterns Treated as Injection by Expanded Classifier

**What goes wrong:** The L3 boundary tokens use HMAC-SHA256 hex strings embedded in prompt text (e.g., `[BOUNDARY:a1b2c3d4...]`). If injection pattern expansion adds patterns matching hex strings or bracket-delimited tokens, the boundary tokens themselves may be flagged as injection patterns, causing internal prompt assembly to fail or be blocked.

**Prevention:**
- Injection classifier patterns should explicitly exclude the HMAC boundary token format.
- Add boundary token examples to the false-positive test corpus.
- Consider allowlisting the boundary pattern format in the classifier: if `[BOUNDARY:` prefix matches, skip injection scanning for that token.

**Confidence:** MEDIUM -- depends on how aggressively patterns are expanded. Current patterns do not match this format, but expanded ones might.

---

### Pitfall 14: Output Screener PII Pattern Sharing Creates Double-Redaction

**What goes wrong:** Phase 64 (v1.5) wired PII pattern sharing from `blufio-security` into the `OutputScreener`. If injection defense hardening adds new PII-like patterns directly in `blufio-injection` (to cover credential formats), there is a risk of duplicate detection where both `detect_pii()` and the credential regex patterns match the same text, causing double-redaction artifacts like `[REDACTED:email][REDACTED]`.

**Prevention:**
- The credential patterns in `output_screen.rs` (CREDENTIAL_PATTERNS) run AFTER `detect_pii()`. Ensure new credential patterns do not overlap with PII patterns.
- Add tests with strings that match both PII and credential patterns (e.g., an email address in a database connection string) to verify clean single-pass redaction.

**Confidence:** MEDIUM -- the current two-phase approach handles this correctly, but pattern expansion could break it.

---

### Pitfall 15: Benchmark Binary Size Includes Debug Info in Default Release Profile

**What goes wrong:** Cargo's default release profile does not strip debug symbols or enable LTO. The measured binary size includes debug sections, giving a misleadingly large number. Developers then spend effort optimizing the wrong thing.

**Prevention:**
- Ensure the workspace `Cargo.toml` has:
  ```toml
  [profile.release]
  lto = true
  strip = true
  codegen-units = 1
  ```
- Measure binary size ONLY from the stripped release build.
- Document the measurement command: `cargo build --release && wc -c target/release/blufio`.

**Confidence:** HIGH -- standard Rust binary optimization practice.

---

### Pitfall 16: vec0 Does Not Support LIKE/GLOB/NULL in Metadata Filters

**What goes wrong:** Developers add metadata columns to vec0 expecting full SQL filter support. But vec0 metadata columns only support: `=`, `!=`, `>`, `>=`, `<`, `<=`, and `IN`. No `LIKE`, `GLOB`, `IS NULL`, or `IS NOT NULL`. The current `deleted_at IS NULL` filter cannot be directly expressed.

**Prevention:**
- Use `is_deleted INTEGER DEFAULT 0` instead of `deleted_at IS NULL`.
- Use exact match on `status = 'active'` (supported).
- Use exact match on `classification != 'restricted'` (supported via `!=`).
- Document these limitations in the code comments for the vec0 schema.

**Confidence:** HIGH -- confirmed in sqlite-vec metadata documentation: "Regular expressions, pattern matching (LIKE, GLOB), and NULL values are not currently supported."

---

### Pitfall 17: Injection Classifier Regex Compilation Slowdown

**What goes wrong:** The current 11 patterns compile via `LazyLock<RegexSet>` on first use, taking < 1ms. Expanding to 50+ patterns (including Unicode-aware patterns) increases compilation time and memory usage. If patterns use `\p{L}` (Unicode letter class) or lookahead/lookbehind, the `regex` crate's compilation time grows significantly.

**Prevention:**
- Benchmark regex compilation time as part of the injection defense benchmarks.
- Keep patterns using `(?i)` case-insensitive ASCII (fast) rather than `(?i)` with Unicode (`(?iu)`).
- If compilation exceeds 10ms, consider pre-compiling the regex set and caching it more aggressively.
- Monitor the `INJECTION_REGEX_SET` first-access latency with a startup benchmark.

**Confidence:** MEDIUM -- the `regex` crate is efficient for ASCII patterns but can be slow for complex Unicode patterns.

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| sqlite-vec integration | **P1: SQLCipher init order** | Register extension per-connection after PRAGMA key, never via auto_extension |
| sqlite-vec integration | **P2: Bundled SQLite conflict** | Vendor sqlite-vec C source, compile against SQLCipher headers from libsqlite3-sys |
| sqlite-vec integration | **P3: Distance vs similarity** | Convert immediately: `similarity = 1.0 - distance`. Overfetch K*2 for threshold filtering |
| sqlite-vec integration | **P4: External filter failure** | Use vec0 metadata columns for status/classification/is_deleted. No JOINs for filtering |
| sqlite-vec migration | **P5: BLOB format mismatch** | Verify format with test vectors before bulk migration. Roll back on mismatch |
| sqlite-vec migration | **P7: Trigger sync failure** | All vec0 writes in Rust application code, not SQL triggers. Pair with memories table writes |
| sqlite-vec migration | **P10: Single-writer starvation** | Batch migration in 500-row chunks, yield between batches |
| sqlite-vec storage | **P6: Doubled storage** | Plan to drop embedding BLOB column after migration validates. Consider int8 quantization |
| Benchmarking suite | **P8: Artifact measurement** | Use file-backed encrypted SQLite, include ONNX model, strip binary for size checks |
| Benchmarking CI | **P11: Flaky on shared runners** | Use percentage thresholds (20%) not statistical tests. Store baselines in bench_results |
| Benchmarking binary | **P15: Debug info inflates size** | Ensure `strip = true`, `lto = true` in release profile |
| Injection hardening | **P9: False positive epidemic** | Benign corpus testing. Dry-run first. Track FP rate as metric |
| Injection hardening | **P13: Boundary token self-match** | Allowlist HMAC boundary format. Add to benign corpus |
| Injection hardening | **P14: Double-redaction** | Test overlapping PII + credential patterns for single-pass correctness |
| Injection hardening | **P16: vec0 metadata limits** | Use integer flags not NULLs. Only =, !=, >, <, IN operators |
| Injection hardening | **P17: Regex compile slowdown** | Benchmark compilation. Prefer ASCII patterns over Unicode |

---

## Summary: Top 5 Actions to Prevent Major Issues

1. **NEVER use `sqlite3_auto_extension` with SQLCipher.** Register sqlite-vec per-connection after PRAGMA key. (Pitfalls 1, 2)

2. **Convert vec0 distance to similarity immediately.** The pipeline expects cosine similarity (0-1, higher=better), not cosine distance (0-2, lower=better). (Pitfall 3)

3. **Use vec0 metadata columns for all filters.** External JOINs execute AFTER KNN, defeating WHERE clauses on status/classification/deleted. (Pitfalls 4, 16)

4. **Maintain a false-positive test corpus.** Every new injection pattern must pass benign corpus validation before promotion from dry_run. (Pitfall 9)

5. **Batch the migration, do not starve the writer.** 500-row chunks with yields prevent agent downtime during vec0 population. (Pitfall 10)

---

## Sources

- [sqlite-vec Rust documentation](https://alexgarcia.xyz/sqlite-vec/rust.html) -- extension registration pattern
- [sqlite-vec API reference](https://alexgarcia.xyz/sqlite-vec/api-reference.html) -- vector types, distance functions
- [sqlite-vec metadata and filtering](https://alexgarcia.xyz/blog/2024/sqlite-vec-metadata-release/index.html) -- metadata columns, partition keys, filtering constraints
- [sqlite-vec GitHub repository](https://github.com/asg017/sqlite-vec) -- source code, issues, architecture
- [sqlite-vec filter limitation (issue #196)](https://github.com/asg017/sqlite-vec/issues/196) -- KNN query planning with JOIN/WHERE
- [SQLCipher API documentation](https://www.zetetic.net/sqlcipher/sqlcipher-api/) -- PRAGMA key must be first statement
- [rusqlite crate](https://crates.io/crates/rusqlite) -- bundled-sqlcipher feature, version 0.37
- [sqlite-vec crate](https://crates.io/crates/sqlite-vec) -- Rust bindings, version 0.1.6
- [OWASP LLM Prompt Injection Prevention](https://cheatsheetseries.owasp.org/cheatsheets/LLM_Prompt_Injection_Prevention_Cheat_Sheet.html) -- defense-in-depth, regex limitations
- [Google prompt injection defense](https://security.googleblog.com/2025/06/mitigating-prompt-injection-attacks.html) -- layered defense strategy
- [Criterion.rs documentation](https://bheisler.github.io/criterion.rs/book/) -- benchmarking methodology
- [peakmem-alloc](https://github.com/PSeitz/peakmem-alloc) -- peak memory measurement in Rust
- [Measuring Memory Usage in Rust](https://rust-analyzer.github.io/blog/2020/12/04/measuring-memory-usage-in-rust.html) -- RSS vs allocator metrics
