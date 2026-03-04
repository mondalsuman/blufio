# Phase 28: Close Audit Gaps - Research

**Researched:** 2026-03-04
**Domain:** Documentation/process gap closure (Cargo feature flag fix, verification file creation, traceability update)
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Tech debt scope**
- Required gap closures ONLY — the 5 actions specified by the audit
- Low-severity tech debt items (encrypt.rs duplication, optional dependency hygiene) are NOT in scope
- Those can be addressed in a future maintenance phase if needed

**Verification depth**
- Cross-reference existing audit evidence (integration checker results) as the basis
- Verify each requirement against what the audit already confirmed is wired
- No need for independent re-inspection — audit already confirmed 29/30 requirements are functionally wired

**CIPH-01 fix validation**
- Change `bundled-sqlcipher` to `bundled-sqlcipher-vendored-openssl` in workspace Cargo.toml
- Run `cargo check` after the change to validate it compiles
- If build fails, investigate and fix before proceeding

### Claude's Discretion
- Verification file format and structure (follow existing patterns from 23-VERIFICATION.md, 24-VERIFICATION.md, 26-VERIFICATION.md)
- Order of gap closure operations
- Exact wording in REQUIREMENTS.md checkbox updates

### Deferred Ideas (OUT OF SCOPE)
- encrypt.rs integrity check duplication fix (low severity) — future maintenance
- Optional dependency hygiene for blufio-storage (low severity) — future maintenance
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CIPH-01 | rusqlite uses bundled-sqlcipher-vendored-openssl feature flag | Fix Cargo.toml line 29; verify with `cargo check`; create 25-VERIFICATION.md confirming fix |
| CIPH-02 | PRAGMA key is first statement on every connection | Already wired — integration audit confirmed; document in 25-VERIFICATION.md with evidence |
| CIPH-03 | Encryption key from BLUFIO_DB_KEY env var | Already wired — integration audit confirmed; document in 25-VERIFICATION.md |
| CIPH-04 | Connection opener verifies key with immediate SELECT | Already wired — integration audit confirmed; document in 25-VERIFICATION.md |
| CIPH-05 | Centralized open_connection() factory in blufio-storage | Already wired — integration audit confirmed; document in 25-VERIFICATION.md |
| CIPH-06 | blufio db encrypt CLI migrates plaintext DB to encrypted | Already wired — integration audit confirmed; document in 25-VERIFICATION.md |
| CIPH-07 | Backup/restore pass encryption key to both connections | Already wired — integration audit confirmed; document in 25-VERIFICATION.md |
| CIPH-08 | blufio doctor reports encryption status | Already wired — integration audit confirmed; document in 25-VERIFICATION.md |
| SIGN-01 | Minisign public key embedded as compile-time constant | Already verified in 26-VERIFICATION.md (PASS); populate 26-01-SUMMARY.md frontmatter |
| SIGN-02 | Signature verified before file operations | Already verified in 26-VERIFICATION.md (PASS); populate 26-01-SUMMARY.md frontmatter |
| SIGN-03 | Verification failure aborts with clear error | Already verified in 26-VERIFICATION.md (PASS); populate 26-01-SUMMARY.md frontmatter |
| SIGN-04 | blufio verify CLI command | Already verified in 26-VERIFICATION.md (PASS); populate 26-02-SUMMARY.md frontmatter |
| UPDT-01 | blufio update checks GitHub Releases API | Already wired — integration audit confirmed; document in 27-VERIFICATION.md |
| UPDT-02 | Downloads platform-appropriate binary + .minisig | Already wired — integration audit confirmed; document in 27-VERIFICATION.md |
| UPDT-03 | Downloaded binary is Minisign-verified before file ops | Already wired — integration audit confirmed; document in 27-VERIFICATION.md |
| UPDT-04 | Current binary backed up before atomic swap | Already wired — integration audit confirmed; document in 27-VERIFICATION.md |
| UPDT-05 | Post-swap health check runs blufio doctor | Already wired — integration audit confirmed; document in 27-VERIFICATION.md |
| UPDT-06 | blufio update rollback reverts to pre-update binary | Already wired — integration audit confirmed; document in 27-VERIFICATION.md |
| UPDT-07 | blufio update --check reports version without downloading | Already wired — integration audit confirmed; document in 27-VERIFICATION.md |
| UPDT-08 | Update requires --yes or interactive confirmation | Already wired — integration audit confirmed; document in 27-VERIFICATION.md |
| SYSD-01 | Binary sends sd_notify READY=1 after init | Already verified in 24-VERIFICATION.md (PASS); update REQUIREMENTS.md checkbox only |
| SYSD-02 | Binary sends sd_notify STOPPING=1 on shutdown | Already verified in 24-VERIFICATION.md (PASS); update REQUIREMENTS.md checkbox only |
| SYSD-03 | Watchdog ping at half WatchdogSec | Already verified in 24-VERIFICATION.md (PASS); update REQUIREMENTS.md checkbox only |
| SYSD-04 | systemd unit file uses Type=notify with WatchdogSec=30 | Already verified in 24-VERIFICATION.md (PASS); update REQUIREMENTS.md checkbox only |
| SYSD-05 | sd_notify silent no-op on non-systemd | Already verified in 24-VERIFICATION.md (PASS); update REQUIREMENTS.md checkbox only |
| SYSD-06 | Binary sends STATUS= during startup phases | Already verified in 24-VERIFICATION.md (PASS); update REQUIREMENTS.md checkbox only |
</phase_requirements>

---

## Summary

Phase 28 is a gap-closure phase, not an implementation phase. The v1.2-MILESTONE-AUDIT.md identified that 29 of 30 requirements are functionally wired in code, but the process documentation has five gaps: one real implementation deviation (CIPH-01 feature flag name), two missing VERIFICATION.md files (phases 25 and 27), and empty `requirements_completed` frontmatter in four SUMMARY files (26-01, 26-02, 27-01, 27-02). Additionally, 26 REQUIREMENTS.md checkboxes remain unchecked despite those requirements being verified or wired.

The work is entirely mechanical: one Cargo.toml line change followed by a `cargo check` verification, two new VERIFICATION.md files authored from audit evidence, two SUMMARY frontmatter patches, and a batch of checkbox/status updates in REQUIREMENTS.md. No new code is written. No new tests are required. The audit has already done the investigative work — this phase converts audit findings into formal documentation.

The key risk is getting the VERIFICATION.md files right: they must follow the exact format established by 23-VERIFICATION.md, 24-VERIFICATION.md, and 26-VERIFICATION.md, cite concrete code-level evidence per requirement, and reach the correct PASS/FAIL/WARN verdict. For CIPH-01 specifically, the verification must show both the old value, the fix applied, and the `cargo check` result confirming the feature flag now compiles correctly.

**Primary recommendation:** Execute the 5 closure actions in order — CIPH-01 fix first (validates the feature flag before verification files are written), then 25-VERIFICATION.md, then 27-VERIFICATION.md, then REQUIREMENTS.md checkboxes, then SUMMARY frontmatter patches.

---

## Standard Stack

### Core
| Tool | Version | Purpose | Why Standard |
|------|---------|---------|--------------|
| Cargo.toml | workspace | rusqlite feature flag — single source of truth for the whole workspace | Rust workspace dependency management |
| cargo check | stable | Validates compilation without full build | Faster than `cargo build`; confirms feature flag resolves |
| Markdown (VERIFICATION.md) | project format | Formal per-requirement evidence document | Established by phases 23, 24, 26 |

### Supporting
| Tool | Version | Purpose | When to Use |
|------|---------|---------|-------------|
| cargo build | stable | Full compilation | If `cargo check` passes but runtime behavior needs confirmation |
| grep / Rust source inspection | - | Finding evidence for verification file entries | Locating function names, line numbers, call sites |

### No New Libraries
This phase introduces no new dependencies. It patches one existing dependency line in Cargo.toml.

---

## Architecture Patterns

### Verification File Structure (established pattern)

Based on 23-VERIFICATION.md, 24-VERIFICATION.md, and 26-VERIFICATION.md:

```markdown
---
phase: {phase-slug}
status: passed
verified: {date}
---

# Phase {N}: {Name} — Verification Report

## Phase Goal
{Goal from phase context}

## Requirement Verification

| ID | Description | Status | Evidence |
|----|-------------|--------|----------|
| CIPH-01 | {description} | PASS | {code location + line number} |
...

## Must-Have Truths Verification

| Truth | Status | Evidence |
|-------|--------|----------|
| {from plan must_haves.truths} | VERIFIED | {code evidence} |

## Artifacts

| File | Purpose |
|------|---------|
| {file path} | {what it contains} |

## Score
{N}/{N} requirements verified. Phase goal achieved.
```

The 24-VERIFICATION.md is the most comprehensive template — it includes requirement verification, plan-level must-have truths, cross-cutting invariants, and artifact depth checks. The 26-VERIFICATION.md is simpler but adequate. Follow 24-VERIFICATION.md structure as the baseline for 25-VERIFICATION.md (8 requirements, multiple plans) and a simpler version of 26-VERIFICATION.md for 27-VERIFICATION.md (8 requirements, 2 plans).

### SUMMARY Frontmatter Pattern (established pattern)

From 25-01-SUMMARY.md and 24-01-SUMMARY.md, the `requirements_completed` field is:

```yaml
---
phase: {phase-slug}
plan: {01|02}
...
requirements-completed: [REQ-ID-1, REQ-ID-2, REQ-ID-3]
...
---
```

Note: The field uses `requirements-completed` (hyphen, not underscore). The audit refers to it as `requirements_completed` in prose but the actual YAML key uses a hyphen (confirmed from 25-01-SUMMARY.md line 39: `requirements-completed: [CIPH-01, CIPH-02, CIPH-03]`).

**Assignment of requirements to plans for Phase 26:**
- 26-01-SUMMARY.md: SIGN-01, SIGN-02, SIGN-03 (from 26-01-PLAN.md requirements field: SIGN-01, SIGN-02, SIGN-03)
- 26-02-SUMMARY.md: SIGN-04 (from 26-02-PLAN.md which added the `blufio verify` CLI)

**Assignment of requirements to plans for Phase 27:**
- 27-01-SUMMARY.md: UPDT-01, UPDT-02, UPDT-03, UPDT-07, UPDT-08 (from 27-01-PLAN.md requirements field)
- 27-02-SUMMARY.md: UPDT-04, UPDT-05, UPDT-06 (from 27-02 body text coverage — backup, health check, rollback)

### REQUIREMENTS.md Update Pattern

The file uses two locations per requirement:
1. The checkbox line: `- [ ] **REQ-ID**:` → `- [x] **REQ-ID**:`
2. The traceability table: `| REQ-ID | Phase N → 28 | Pending |` → `| REQ-ID | Phase N | Complete |`

The traceability table currently shows `Phase N → 28` for all pending requirements. After gap closure, the redirect to `→ 28` should be removed since the requirement is now formally complete.

Requirements to update:
- SYSD-01..06: checkbox + traceability (VERIFICATION.md already exists in 24)
- CIPH-01..08: checkbox + traceability (25-VERIFICATION.md will be created)
- SIGN-01..04: checkbox + traceability (26-VERIFICATION.md already exists)
- UPDT-01..08: checkbox + traceability (27-VERIFICATION.md will be created)

Total: 26 checkboxes + 26 traceability rows = 52 edits in REQUIREMENTS.md.

### Anti-Patterns to Avoid

- **Writing VERIFICATION.md from memory instead of code inspection:** The verification file MUST cite actual code locations (file path, line number or function name). The audit already did the code inspection — use its evidence verbatim, then verify locations are still accurate.
- **Guessing which requirements go in which SUMMARY:** Use the `requirements:` field in each PLAN.md frontmatter to assign accurately. 26-01-PLAN.md explicitly lists `requirements: [SIGN-01, SIGN-02, SIGN-03]`.
- **Using underscore instead of hyphen in YAML key:** The actual field is `requirements-completed:` (with hyphen), as seen in 25-01-SUMMARY.md, 24-01-SUMMARY.md.
- **Running `cargo build` before `cargo check`:** `cargo check` is faster and sufficient to validate the feature flag change.
- **Leaving REQUIREMENTS.md traceability table with "→ 28" redirect:** The redirect notation means "originally assigned to Phase N, completed in Phase 28." After completion, update to just `Phase N` or `Phase 25 (closed in 28)` to keep historical accuracy.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Evidence for verification files | Re-inspecting code from scratch | v1.2-MILESTONE-AUDIT.md evidence section | Audit already found all call sites with line numbers |
| Feature flag validity check | Manual Cargo.lock inspection | `cargo check` | Compiler validates the feature flag exists and compiles |
| SUMMARY frontmatter discovery | Parsing YAML manually | Reading the plan's `requirements:` frontmatter field | Plan frontmatter is the authoritative assignment source |

**Key insight:** The audit did all the investigative work. Phase 28 is transcription + one code fix, not research. Use the audit's evidence directly.

---

## Common Pitfalls

### Pitfall 1: Wrong YAML Key Name for Frontmatter
**What goes wrong:** Writing `requirements_completed:` (underscore) instead of `requirements-completed:` (hyphen) in SUMMARY files.
**Why it happens:** The audit and planning docs refer to it as `requirements_completed` in prose descriptions, but the actual YAML key observed in 24-01-SUMMARY.md and 25-01-SUMMARY.md uses a hyphen.
**How to avoid:** Check 25-01-SUMMARY.md line 39 (`requirements-completed: [CIPH-01, CIPH-02, CIPH-03]`) as the canonical example before writing.
**Warning signs:** If grep for `requirements-completed` in an existing SUMMARY returns nothing, the field format is wrong.

### Pitfall 2: Incorrect Plan-to-Requirement Assignment
**What goes wrong:** Assigning SIGN-04 to 26-01-SUMMARY.md instead of 26-02-SUMMARY.md (or similar cross-plan error).
**Why it happens:** The audit describes gaps at the phase level, not plan level.
**How to avoid:** Read the `requirements:` field in each PLAN.md frontmatter explicitly before adding to SUMMARY frontmatter. 26-01-PLAN.md has `[SIGN-01, SIGN-02, SIGN-03]`. 27-01-PLAN.md has `[UPDT-01, UPDT-02, UPDT-03, UPDT-07, UPDT-08]`.
**Warning signs:** A requirement appears in both SUMMARY frontmatter files, or appears in neither.

### Pitfall 3: CIPH-01 Feature Flag Build Failure
**What goes wrong:** Changing `bundled-sqlcipher` to `bundled-sqlcipher-vendored-openssl` and getting a compilation failure due to missing OpenSSL build dependencies on the current machine.
**Why it happens:** The vendored-openssl feature compiles OpenSSL from source (C code). On some systems, build tools (cc, cmake) may be missing or misconfigured.
**How to avoid:** Run `cargo check` (not `cargo build`) immediately after the change. If it fails, investigate the error before proceeding.
**Warning signs:** Error messages mentioning `openssl-sys`, `cc`, or `cmake` during `cargo check`.
**Note from Phase 25 plan:** "On macOS this should work out of the box with the vendored flag." However, the feature was supposedly changed in Phase 25 but the Cargo.toml still shows `bundled-sqlcipher` — meaning either the change was reverted or never actually applied. This makes the compile risk real.

### Pitfall 4: VERIFICATION.md Written Without Concrete Evidence
**What goes wrong:** Verification file says "PASS" without citing the actual function name, file path, or line number.
**Why it happens:** Rushing to document without inspecting code.
**How to avoid:** For each requirement in the verification file, cite the specific function and file from the audit's evidence. The audit lists exact call sites (e.g., "verify_download() calls blufio_verify::verify_signature before file ops").
**Warning signs:** Evidence cells say "confirmed" or "implemented" without a code location.

### Pitfall 5: REQUIREMENTS.md Traceability Table Format Mismatch
**What goes wrong:** The traceability table shows `Phase 24 → 28` for SYSD requirements after the update, but it should reflect completion.
**Why it happens:** Unclear what the final format should look like.
**How to avoid:** Look at BKUP-01..04 rows as the template for "Complete" requirements:
```
| BKUP-01 | Phase 23 | Complete |
```
Update all 26 pending rows to follow this pattern (remove the `→ 28` redirect, set status to `Complete`).

---

## Code Examples

### CIPH-01 Fix — Cargo.toml Before and After

```toml
# Before (current state confirmed at Cargo.toml line 29):
rusqlite = { version = "0.37", features = ["bundled-sqlcipher"] }

# After:
rusqlite = { version = "0.37", features = ["bundled-sqlcipher-vendored-openssl"] }
```

Validation command:
```bash
cd /Users/suman/projects/github/blufio && cargo check 2>&1 | tail -10
```

### REQUIREMENTS.md Checkbox Update Pattern

```markdown
# Before:
- [ ] **SYSD-01**: Binary sends sd_notify READY=1 after all initialization completes

# After:
- [x] **SYSD-01**: Binary sends sd_notify READY=1 after all initialization completes
```

### REQUIREMENTS.md Traceability Table Update Pattern

```markdown
# Before:
| SYSD-01 | Phase 24 → 28 | Pending |

# After:
| SYSD-01 | Phase 24 | Complete |
```

### SUMMARY Frontmatter Patch Pattern for 26-01-SUMMARY.md

The 26-01-SUMMARY.md currently has NO frontmatter (it starts directly with `# Plan 26-01 Summary`). The frontmatter must be added. Follow the 24-01-SUMMARY.md/25-01-SUMMARY.md pattern:

```yaml
---
phase: 26-minisign-signature-verification
plan: 01
subsystem: security
tags: [minisign, signature, verification, embedded-key]

requirements-completed: [SIGN-01, SIGN-02, SIGN-03]

completed: 2026-03-03
---
```

Similarly for 26-02-SUMMARY.md:
```yaml
---
phase: 26-minisign-signature-verification
plan: 02
subsystem: security
tags: [minisign, cli, verify-command]

requirements-completed: [SIGN-04]

completed: 2026-03-04
---
```

For 27-01-SUMMARY.md:
```yaml
---
phase: 27-self-update-with-rollback
plan: 01
subsystem: update
tags: [self-update, github-releases, download, minisign]

requirements-completed: [UPDT-01, UPDT-02, UPDT-03, UPDT-07, UPDT-08]

completed: 2026-03-04
---
```

For 27-02-SUMMARY.md:
```yaml
---
phase: 27-self-update-with-rollback
plan: 02
subsystem: update
tags: [self-update, backup, atomic-swap, health-check, rollback]

requirements-completed: [UPDT-04, UPDT-05, UPDT-06]

completed: 2026-03-04
---
```

### 25-VERIFICATION.md Structure (abbreviated example per requirement)

```markdown
---
phase: 25-sqlcipher-database-encryption
status: passed
verified: 2026-03-04
---

# Phase 25: SQLCipher Database Encryption — Verification Report

## Phase Goal
Database contents are encrypted at rest using SQLCipher. All database consumers go through a centralized connection factory that transparently applies the encryption key.

## Requirement Verification

| ID | Description | Status | Evidence |
|----|-------------|--------|----------|
| CIPH-01 | rusqlite uses bundled-sqlcipher-vendored-openssl feature flag | PASS | Cargo.toml line 29 changed from `bundled-sqlcipher` to `bundled-sqlcipher-vendored-openssl`; `cargo check` exits 0 |
| CIPH-02 | PRAGMA key is first statement on every connection | PASS | `apply_encryption_key()` called as first `conn.call()` in `open_connection()` and `open_connection_sync()` in `crates/blufio-storage/src/database.rs` |
| CIPH-03 | Key from BLUFIO_DB_KEY env var | PASS | `apply_encryption_key()` reads `std::env::var("BLUFIO_DB_KEY")` in `database.rs` |
| CIPH-04 | Key correctness verified with SELECT after PRAGMA key | PASS | `verify_key()` runs `SELECT count(*) FROM sqlite_master` immediately after `apply_encryption_key()` |
| CIPH-05 | Centralized open_connection() factory used by all consumers | PASS | Integration checker confirmed: zero `tokio_rusqlite::Connection::open` in production code outside factory |
| CIPH-06 | blufio db encrypt CLI migrates plaintext to encrypted | PASS | `Commands::Db -> DbCommands::Encrypt -> encrypt::run_encrypt()` in `crates/blufio/src/main.rs` |
| CIPH-07 | Backup/restore pass key to both connections | PASS | `backup.rs` uses `open_connection_sync()` for all file-based connections |
| CIPH-08 | blufio doctor reports encryption status | PASS | `check_encryption()` in `crates/blufio/src/doctor.rs` with 4-way diagnostic |

## Score
8/8 requirements verified. Phase goal achieved.
```

### 27-VERIFICATION.md Structure (abbreviated)

```markdown
---
phase: 27-self-update-with-rollback
status: passed
verified: 2026-03-04
---

# Phase 27: Self-Update with Rollback — Verification Report

## Phase Goal
Operator can update the Blufio binary in-place with Minisign-verified downloads from GitHub Releases, with automatic rollback on failure.

## Requirement Verification

| ID | Description | Status | Evidence |
|----|-------------|--------|----------|
| UPDT-01 | blufio update checks GitHub Releases API | PASS | `fetch_latest_release()` in `crates/blufio/src/update.rs` queries GitHub Releases API |
| UPDT-02 | Downloads platform binary + .minisig | PASS | `download_to_temp()` downloads platform binary and `.minisig` to temp files |
| UPDT-03 | Binary Minisign-verified before file ops | PASS | `verify_download()` calls `blufio_verify::verify_signature()` before `backup_current()`/`self_replace()` |
| UPDT-04 | Current binary backed up before atomic swap | PASS | `backup_current()` + `self_replace::self_replace()` in `update.rs` |
| UPDT-05 | Post-swap health check runs blufio doctor | PASS | `health_check()` spawns `blufio doctor` subprocess with 30s timeout |
| UPDT-06 | blufio update rollback reverts from .bak | PASS | `run_rollback()` -> `do_rollback()` renames `.bak` back to binary path |
| UPDT-07 | blufio update --check reports version without download | PASS | `run_check()` fetches latest version and compares without downloading |
| UPDT-08 | Update requires --yes or interactive confirmation | PASS | `confirm_update()` checks `IsTerminal` and prompts `[y/N]`; `--yes` flag bypasses |

## Score
8/8 requirements verified. Phase goal achieved.
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `bundled-sqlcipher` feature flag | `bundled-sqlcipher-vendored-openssl` | Phase 25 (nominally) — Phase 28 (actual fix) | Hermetic builds with vendored OpenSSL; no system OpenSSL dependency |
| Process documentation gap | Formal VERIFICATION.md + checked traceability | Phase 28 | All 30 v1.2 requirements formally tracked in 3-source cross-reference |

---

## Open Questions

1. **Was `bundled-sqlcipher-vendored-openssl` ever actually set in Phase 25?**
   - What we know: 25-01-SUMMARY.md claims the change was made. The actual Cargo.toml currently shows `bundled-sqlcipher`. Either the change was made and reverted, or the SUMMARY was written incorrectly.
   - What's unclear: Whether `cargo check` with the corrected flag will succeed immediately on the current machine without build tool issues.
   - Recommendation: Run `cargo check` as the very first action after the edit. If it fails, diagnose before writing verification files that depend on CIPH-01 being fixed.

2. **Do 26-01-SUMMARY.md and 26-02-SUMMARY.md have any existing frontmatter?**
   - What we know: The files exist and have no frontmatter (they start with `# Plan 26-01 Summary`).
   - What's unclear: Whether adding frontmatter as a prepend will break any tooling that reads these files.
   - Recommendation: Check how other tools process these files. Given that 25-01-SUMMARY.md has full frontmatter and is processed normally by the GSD workflow, prepending frontmatter to 26-01 and 26-02 is safe.

3. **Do the REQUIREMENTS.md traceability rows keep "→ 28" after Phase 28 closes?**
   - What we know: BKUP-01..04 rows show `Phase 23 | Complete` without any redirect notation.
   - Recommendation: Remove the `→ 28` redirect notation in the traceability table. Change `Phase 24 → 28` to `Phase 24`, `Phase 25 → 28` to `Phase 25`, etc., and set Status to `Complete`. This follows the BKUP pattern exactly.

---

## Sources

### Primary (HIGH confidence)
- `/Users/suman/projects/github/blufio/.planning/v1.2-MILESTONE-AUDIT.md` — Gap closure actions, integration checker evidence for all 30 requirements
- `/Users/suman/projects/github/blufio/.planning/phases/23-backup-integrity-verification/23-VERIFICATION.md` — Template: verification file format with must-have truths
- `/Users/suman/projects/github/blufio/.planning/phases/24-sd-notify-integration/24-VERIFICATION.md` — Template: most comprehensive verification file format
- `/Users/suman/projects/github/blufio/.planning/phases/26-minisign-signature-verification/26-VERIFICATION.md` — Template: simpler format for smaller phases
- `/Users/suman/projects/github/blufio/.planning/phases/25-sqlcipher-database-encryption/25-01-SUMMARY.md` — Canonical `requirements-completed:` YAML key format
- `/Users/suman/projects/github/blufio/.planning/phases/27-self-update-with-rollback/27-01-PLAN.md` — Authoritative UPDT-01/02/03/07/08 plan assignment
- `/Users/suman/projects/github/blufio/.planning/phases/27-self-update-with-rollback/27-02-SUMMARY.md` — UPDT-04/05/06 requirement coverage text
- `/Users/suman/projects/github/blufio/.planning/phases/26-minisign-signature-verification/26-01-SUMMARY.md` — Confirms no frontmatter exists yet
- `/Users/suman/projects/github/blufio/Cargo.toml` line 29 — Confirmed: `bundled-sqlcipher` (not `bundled-sqlcipher-vendored-openssl`)
- `/Users/suman/projects/github/blufio/.planning/REQUIREMENTS.md` — Confirmed: 26 checkboxes unchecked, traceability rows show `Pending`

### Secondary (MEDIUM confidence)
- 27-02-SUMMARY.md body text "Requirement coverage" section — UPDT-04/05/06 assignments inferred from function-to-requirement mapping

---

## Metadata

**Confidence breakdown:**
- Gap identification: HIGH — directly from v1.2-MILESTONE-AUDIT.md with concrete line numbers and function names
- VERIFICATION.md format: HIGH — reverse-engineered from three existing files (23, 24, 26)
- SUMMARY frontmatter format: HIGH — directly observed from 24-01, 25-01, 25-02 SUMMARY files
- REQUIREMENTS.md update pattern: HIGH — directly observed from existing BKUP-01..04 rows as complete examples
- CIPH-01 compile risk: MEDIUM — the feature flag existed historically (Phase 25 plan prescribed it) but may have build tool dependencies

**Research date:** 2026-03-04
**Valid until:** 2026-03-11 (7 days — depends only on local file state, not external services)
