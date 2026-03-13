# Requirements: Blufio

**Defined:** 2026-03-13
**Core Value:** An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file.

## v1.6 Requirements

Requirements for v1.6 Performance & Scalability Validation. Each maps to roadmap phases.

### Vector Search (sqlite-vec)

- [x] **VEC-01**: Memory store uses sqlite-vec vec0 virtual table for disk-backed KNN vector search instead of in-memory brute-force cosine similarity
- [x] **VEC-02**: sqlite-vec integrates with SQLCipher — vec0 data encrypted at rest alongside existing memories table
- [ ] **VEC-03**: vec0 metadata columns filter status='active' and classification!='restricted' during KNN search, not post-query
- [ ] **VEC-04**: Existing BLOB embeddings migrate to vec0 virtual table via batched migration (500-row chunks) with rollback strategy
- [ ] **VEC-05**: Hybrid retrieval (BM25 + vec0 KNN + RRF fusion + temporal decay + importance boost + MMR diversity) preserved and functionally identical to pre-migration
- [ ] **VEC-06**: Eviction (batch_evict) and soft-delete operations sync across both memories and vec0 tables in single transaction
- [ ] **VEC-07**: vec0 partition key by session_id enables faster within-session vector search
- [ ] **VEC-08**: vec0 auxiliary columns eliminate JOIN to memories table for search result retrieval (single-query path)

### Injection Defense Hardening

- [ ] **INJ-01**: Sanitization pre-pass normalizes Unicode (NFKC), strips zero-width characters, and maps homoglyphs before pattern matching
- [ ] **INJ-02**: Injection classifier detects base64-encoded payloads, decodes them, and re-scans decoded content
- [ ] **INJ-03**: Pattern set expanded from 11 to ~25 covering prompt leaking, jailbreak keywords, delimiter manipulation, and encoding obfuscation
- [ ] **INJ-04**: Indirect injection patterns detect instructions hidden in HTML comments, markdown, and JSON content from tool outputs
- [ ] **INJ-05**: Multi-language injection patterns cover French, German, Spanish, Chinese, and Japanese attack vectors
- [ ] **INJ-06**: Configurable severity weights via TOML config allow operators to tune per-category detection thresholds
- [ ] **INJ-07**: Canary token planted in system prompt detected if echoed in LLM output, indicating prompt leaking attack
- [ ] **INJ-08**: Benign message corpus (100+ messages) validates all patterns have acceptable false positive rate before production promotion

### Performance Benchmarking

- [ ] **PERF-01**: Binary size measured and tracked against <50MB target, with per-crate breakdown via cargo-bloat
- [ ] **PERF-02**: Memory RSS profiled for idle (target 50-80MB) and under-load (target 100-200MB) using jemalloc stats
- [ ] **PERF-03**: Criterion benchmarks compare vec0 KNN vs in-memory cosine at 100, 1K, 5K, and 10K entries
- [ ] **PERF-04**: Injection classifier throughput benchmarked at 1KB, 5KB, and 10KB input sizes
- [ ] **PERF-05**: End-to-end hybrid retrieval benchmark measures full pipeline (embed -> vec0 -> BM25 -> RRF -> MMR)
- [ ] **PERF-06**: Comparative benchmark vs OpenClaw validates memory usage and token efficiency claims with reproducible numbers
- [ ] **PERF-07**: CI regression baselines established — benchmarks fail if performance degrades beyond 20% threshold

## Future Requirements

Deferred to future release. Tracked but not in current roadmap.

### Vector Search

- **VEC-F01**: ANN indexes (HNSW/IVF) for 100K+ scale vector search
- **VEC-F02**: Quantized vectors (int8/bit) for 4x-32x storage compression

### Injection Defense

- **INJ-F01**: ML-based injection classifier using ONNX model for semantic detection
- **INJ-F02**: Multimodal injection detection for image-based attacks

### Performance

- **PERF-F01**: Load testing suite (wrk/k6) for HTTP gateway performance
- **PERF-F02**: jemalloc per-subsystem allocation tracking via arenas

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| GPU-accelerated vector search | CUDA/ROCm adds 100MB+ to binary, violates edge deployment model |
| Separate vector database (Chroma/Pinecone) | Violates single-binary, single-file deployment model |
| Real-time adversarial training | Pattern poisoning risk, maintenance burden exceeds benefit |
| flamegraph in release binary | Adds size and performance overhead to production builds |
| ML injection classifier | ONNX model weight + inference latency; regex is sufficient per OWASP guidance |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| VEC-01 | Phase 65 | Complete |
| VEC-02 | Phase 65 | Complete |
| VEC-03 | Phase 65 | Pending |
| VEC-04 | Phase 67 | Pending |
| VEC-05 | Phase 67 | Pending |
| VEC-06 | Phase 67 | Pending |
| VEC-07 | Phase 67 | Pending |
| VEC-08 | Phase 67 | Pending |
| INJ-01 | Phase 66 | Pending |
| INJ-02 | Phase 66 | Pending |
| INJ-03 | Phase 66 | Pending |
| INJ-04 | Phase 66 | Pending |
| INJ-05 | Phase 66 | Pending |
| INJ-06 | Phase 66 | Pending |
| INJ-07 | Phase 66 | Pending |
| INJ-08 | Phase 66 | Pending |
| PERF-01 | Phase 68 | Pending |
| PERF-02 | Phase 68 | Pending |
| PERF-03 | Phase 68 | Pending |
| PERF-04 | Phase 68 | Pending |
| PERF-05 | Phase 68 | Pending |
| PERF-06 | Phase 68 | Pending |
| PERF-07 | Phase 68 | Pending |

**Coverage:**
- v1.6 requirements: 23 total
- Mapped to phases: 23
- Unmapped: 0

---
*Requirements defined: 2026-03-13*
*Last updated: 2026-03-13 after roadmap creation (traceability populated)*
