# Roadmap: Blufio

## Milestones

- ✅ **v1.0 MVP** — Phases 1-14 (shipped 2026-03-02)
- ✅ **v1.1 MCP Integration** — Phases 15-22 (shipped 2026-03-03)
- 🚧 **v1.2 Production Hardening** — Phases 23-27 (in progress)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1-14) — SHIPPED 2026-03-02</summary>

- [x] Phase 1: Project Foundation & Workspace (2/2 plans) — completed 2026-02-28
- [x] Phase 2: Persistence & Security Vault (2/2 plans) — completed 2026-02-28
- [x] Phase 3: Agent Loop & Telegram (4/4 plans) — completed 2026-03-01
- [x] Phase 4: Context Engine & Cost Tracking (3/3 plans) — completed 2026-03-01
- [x] Phase 5: Memory & Embeddings (3/3 plans) — completed 2026-03-01
- [x] Phase 6: Model Routing & Smart Heartbeats (3/3 plans) — completed 2026-03-01
- [x] Phase 7: WASM Skill Sandbox (4/4 plans) — completed 2026-03-01
- [x] Phase 8: Plugin System & Gateway (3/3 plans) — completed 2026-03-01
- [x] Phase 9: Production Hardening (3/3 plans) — completed 2026-03-01
- [x] Phase 10: Multi-Agent & Final Integration (3/3 plans) — completed 2026-03-01
- [x] Phase 11: Fix Critical Integration Bugs (4/4 plans) — completed 2026-03-01
- [x] Phase 12: Verify Unverified Phases (5/5 plans) — completed 2026-03-01
- [x] Phase 13: Sync Traceability & Documentation (1/1 plan) — completed 2026-03-02
- [x] Phase 14: Wire Cross-Phase Integration (3/3 plans) — completed 2026-03-02

</details>

<details>
<summary>✅ v1.1 MCP Integration (Phases 15-22) — SHIPPED 2026-03-03</summary>

- [x] Phase 15: MCP Foundation (4/4 plans) — completed 2026-03-02
- [x] Phase 16: MCP Server stdio (3/3 plans) — completed 2026-03-02
- [x] Phase 17: MCP Server HTTP + Resources (5/5 plans) — completed 2026-03-02
- [x] Phase 18: MCP Client (4/4 plans) — completed 2026-03-03
- [x] Phase 19: Integration Testing + Tech Debt (5/5 plans) — completed 2026-03-03
- [x] Phase 20: Verify Phase 15 & 16 Completeness (4/4 plans) — completed 2026-03-03
- [x] Phase 21: Fix MCP Wiring Gaps (4/4 plans) — completed 2026-03-03
- [x] Phase 22: Verify Phase 18 & 19 + Close Traceability (3/3 plans) — completed 2026-03-03

</details>

### 🚧 v1.2 Production Hardening (In Progress)

**Milestone Goal:** Close critical PRD gaps -- systemd readiness, database encryption at rest, supply chain integrity, self-update, and backup verification.

- [ ] **Phase 23: Backup Integrity Verification** - PRAGMA integrity_check after backup and restore with corruption handling
- [ ] **Phase 24: sd_notify Integration** - systemd Type=notify readiness, watchdog pings, and status reporting
- [ ] **Phase 25: SQLCipher Database Encryption** - Encryption at rest with centralized key management and migration CLI
- [ ] **Phase 26: Minisign Signature Verification** - Ed25519 binary signature verification with embedded public key
- [ ] **Phase 27: Self-Update with Rollback** - Version check, download, verify, atomic swap, health check, rollback

## Phase Details

### Phase 23: Backup Integrity Verification
**Goal**: Operator can trust that backups are not silently corrupt and restores produce a valid database
**Depends on**: Nothing (first phase of v1.2; zero new dependencies, uses existing PRAGMA integrity_check pattern from doctor.rs)
**Requirements**: BKUP-01, BKUP-02, BKUP-03, BKUP-04
**Success Criteria** (what must be TRUE):
  1. After `blufio backup` completes, the backup file has been verified with PRAGMA integrity_check and the operator sees integrity status in the output
  2. After `blufio restore` completes, the restored database has been verified with PRAGMA integrity_check and the operator sees integrity status in the output
  3. A backup file that fails integrity_check is automatically deleted and the operator sees a clear error explaining the corruption
  4. Backup and restore output includes both file size and integrity status (e.g., "Backup complete: 5.2 MB, integrity: ok")
**Plans**: 1 plan
  - [ ] 23-01-PLAN.md -- Add integrity check helper, verify backup/restore output, pre-check, post-check, rollback

### Phase 24: sd_notify Integration
**Goal**: systemd knows exactly when Blufio is ready, when it is shutting down, and that it is still alive -- enabling proper Type=notify service management
**Depends on**: Nothing (independent of Phase 23; zero cross-crate impact)
**Requirements**: SYSD-01, SYSD-02, SYSD-03, SYSD-04, SYSD-05, SYSD-06
**Success Criteria** (what must be TRUE):
  1. `systemctl start blufio` transitions to "active (running)" only after all initialization completes (sd_notify READY=1 sent after mux.connect)
  2. `systemctl status blufio` shows startup progress messages (STATUS= sent during initialization phases)
  3. systemd automatically restarts Blufio if the watchdog ping stops arriving (watchdog ping at half the WatchdogSec interval)
  4. `systemctl stop blufio` triggers a clean shutdown sequence (sd_notify STOPPING=1 sent when shutdown begins)
  5. On macOS or Docker (no NOTIFY_SOCKET), all sd_notify calls are silent no-ops -- no errors, no log noise
**Plans**: TBD

### Phase 25: SQLCipher Database Encryption
**Goal**: Operator can encrypt the database at rest so that a stolen disk or backup file reveals nothing without the encryption key
**Depends on**: Phase 23 (migration CLI uses PRAGMA integrity_check to verify export correctness)
**Requirements**: CIPH-01, CIPH-02, CIPH-03, CIPH-04, CIPH-05, CIPH-06, CIPH-07, CIPH-08
**Success Criteria** (what must be TRUE):
  1. With BLUFIO_DB_KEY set, all database files are encrypted at rest -- opening with sqlite3 CLI without the key shows "file is encrypted or is not a database"
  2. `blufio db encrypt` migrates an existing plaintext database to encrypted without data loss (three-file safety strategy: original untouched until verified)
  3. All 6+ database consumers (storage, memory, cost, queue, sessions, vault) use the centralized open_connection() factory -- no raw Connection::open() calls bypass encryption
  4. `blufio doctor` reports encryption status, cipher version, and page size for the database
  5. Backup and restore work correctly with encrypted databases (encryption key passed to both source and destination connections)
**Plans**: TBD

### Phase 26: Minisign Signature Verification
**Goal**: Operator can verify that any Blufio binary or file is authentically signed by the project maintainer
**Depends on**: Nothing (independent; purely additive new module)
**Requirements**: SIGN-01, SIGN-02, SIGN-03, SIGN-04
**Success Criteria** (what must be TRUE):
  1. The Minisign public key is compiled into the binary -- no external key file needed for verification
  2. `blufio verify <file> <signature>` verifies any file against its .minisig signature and reports pass/fail with clear output
  3. Signature verification failure produces a clear, actionable error message that names the file, states what failed, and does not proceed with any file operations
**Plans**: TBD

### Phase 27: Self-Update with Rollback
**Goal**: Operator can update Blufio in place with a single command, with signature verification and automatic rollback if the new binary is broken
**Depends on**: Phase 26 (Minisign verification is a hard prerequisite -- never swap an unverified binary)
**Requirements**: UPDT-01, UPDT-02, UPDT-03, UPDT-04, UPDT-05, UPDT-06, UPDT-07, UPDT-08
**Success Criteria** (what must be TRUE):
  1. `blufio update --check` reports the latest available version from GitHub Releases without downloading anything
  2. `blufio update` downloads the platform-appropriate binary, verifies its Minisign signature, backs up the current binary, performs an atomic swap, and runs `blufio doctor` as a health check -- all in one command
  3. `blufio update rollback` reverts to the pre-update binary that was backed up before the swap
  4. `blufio update` requires explicit confirmation (--yes flag or interactive prompt) before proceeding -- no silent updates
  5. If signature verification fails at any point, the update aborts immediately with a clear error and the current binary is untouched
**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 23 -> 24 -> 25 -> 26 -> 27

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Project Foundation & Workspace | v1.0 | 2/2 | Complete | 2026-02-28 |
| 2. Persistence & Security Vault | v1.0 | 2/2 | Complete | 2026-02-28 |
| 3. Agent Loop & Telegram | v1.0 | 4/4 | Complete | 2026-03-01 |
| 4. Context Engine & Cost Tracking | v1.0 | 3/3 | Complete | 2026-03-01 |
| 5. Memory & Embeddings | v1.0 | 3/3 | Complete | 2026-03-01 |
| 6. Model Routing & Smart Heartbeats | v1.0 | 3/3 | Complete | 2026-03-01 |
| 7. WASM Skill Sandbox | v1.0 | 4/4 | Complete | 2026-03-01 |
| 8. Plugin System & Gateway | v1.0 | 3/3 | Complete | 2026-03-01 |
| 9. Production Hardening | v1.0 | 3/3 | Complete | 2026-03-01 |
| 10. Multi-Agent & Final Integration | v1.0 | 3/3 | Complete | 2026-03-01 |
| 11. Fix Critical Integration Bugs | v1.0 | 4/4 | Complete | 2026-03-01 |
| 12. Verify Unverified Phases | v1.0 | 5/5 | Complete | 2026-03-01 |
| 13. Sync Traceability & Documentation | v1.0 | 1/1 | Complete | 2026-03-02 |
| 14. Wire Cross-Phase Integration | v1.0 | 3/3 | Complete | 2026-03-02 |
| 15. MCP Foundation | v1.1 | 4/4 | Complete | 2026-03-02 |
| 16. MCP Server stdio | v1.1 | 3/3 | Complete | 2026-03-02 |
| 17. MCP Server HTTP + Resources | v1.1 | 5/5 | Complete | 2026-03-02 |
| 18. MCP Client | v1.1 | 4/4 | Complete | 2026-03-03 |
| 19. Integration Testing + Tech Debt | v1.1 | 5/5 | Complete | 2026-03-03 |
| 20. Verify Phase 15 & 16 Completeness | v1.1 | 4/4 | Complete | 2026-03-03 |
| 21. Fix MCP Wiring Gaps | v1.1 | 4/4 | Complete | 2026-03-03 |
| 22. Verify Phase 18 & 19 + Close Traceability | v1.1 | 3/3 | Complete | 2026-03-03 |
| 23. Backup Integrity Verification | v1.2 | 0/0 | Not started | - |
| 24. sd_notify Integration | v1.2 | 0/0 | Not started | - |
| 25. SQLCipher Database Encryption | v1.2 | 0/0 | Not started | - |
| 26. Minisign Signature Verification | v1.2 | 0/0 | Not started | - |
| 27. Self-Update with Rollback | v1.2 | 0/0 | Not started | - |

---
*Roadmap created: 2026-02-28*
*Last updated: 2026-03-03 after v1.2 roadmap creation*
