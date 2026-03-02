# Phase 13: Sync Traceability & Documentation - Research

**Researched:** 2026-03-02
**Domain:** Documentation synchronization (REQUIREMENTS.md traceability table + ROADMAP.md progress table)
**Confidence:** HIGH

## Summary

Phase 13 is a pure documentation sync -- no code changes, no new features, no tooling. The task is to update two files (REQUIREMENTS.md and ROADMAP.md) so their statuses, checkboxes, plan counts, and coverage counts match the verified completion state recorded in VERIFICATION.md files across all 12 phases.

Research identified every discrepancy by cross-referencing each phase's VERIFICATION.md against the current REQUIREMENTS.md traceability table and ROADMAP.md progress table. The result is a complete delta map: 40 requirements currently marked Pending that should be Complete, 2 requirements with stale Phase assignments, and 6 ROADMAP progress table rows with incorrect plan counts or missing checkboxes.

**Primary recommendation:** Execute the sync as a single batch of manual text edits to REQUIREMENTS.md and ROADMAP.md, guided by the delta map produced below. Validate by counting `[x]` checkboxes and `Complete` statuses to confirm they match.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- VERIFICATION.md files are the authoritative source for requirement completion
- Cross-reference with plan SUMMARYs and ROADMAP status for phases without formal verification (Phases 1, 3, 4)
- A requirement is Complete if: phase has VERIFICATION.md with PASS listing it, OR all plan SUMMARYs are Complete AND ROADMAP says Complete
- Evidence drives the traceability table status first, then header checkboxes are made consistent
- Update the traceability table's "Phase" column to reflect where requirements were actually satisfied (e.g., Phase 11 fixed LLM-05, SEC-02, SEC-03)
- Keep binary Pending/Complete -- no Partial status introduced
- Phase 7 requirements: mark individually verified ones (SKILL-01 through SKILL-06, SEC-05, SEC-06) as Complete; leave any unverified ones as Pending
- Header checkboxes and traceability table must always match -- if traceability says Complete, header gets [x]
- Update REQUIREMENTS.md coverage summary with accurate Complete/Pending counts
- Update ROADMAP.md progress table: fix plan counts to reflect actual executed plans
- Update ROADMAP.md top-level phase checkboxes to match progress table completion
- Add completion dates to progress table where verifiable from VERIFICATION.md or SUMMARY timestamps
- Manual text edits -- one-time sync, no scripting or tooling
- After edits, run a quick count validation: count [x] checkboxes vs Complete statuses to confirm they match
- Summarize all changes in the git commit message (no separate changelog file)

### Claude's Discretion
- Exact ordering of edits (which file first, which section first)
- How to handle any ambiguous evidence encountered during the sync
- Formatting consistency choices within the tables

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

## Standard Stack

Not applicable -- this phase involves only manual text edits to Markdown files. No libraries, frameworks, or tooling required.

## Architecture Patterns

### Pattern 1: Evidence-First Delta Map

**What:** Build a complete mapping of current state vs. verified state BEFORE making any edits.
**When to use:** Always -- prevents partial syncs and missed items.

The delta map was constructed by reading all 12 VERIFICATION.md files and comparing their requirements coverage tables against the current REQUIREMENTS.md traceability table.

### Pattern 2: Dual-File Consistency Check

**What:** After editing REQUIREMENTS.md, cross-check with ROADMAP.md to ensure phase statuses align.
**When to use:** After every edit pass.

REQUIREMENTS.md header checkboxes must match traceability table statuses. ROADMAP.md phase checkboxes must match progress table completion. The two files must agree on which phases are complete.

### Anti-Patterns to Avoid
- **Incremental patching without a delta map:** Editing one requirement at a time without a master list leads to missed items. Build the complete delta first, then apply.
- **Trusting ROADMAP plan counts at face value:** Several phases had plans added after initial ROADMAP creation (Phase 3 got plan 04, Phase 7 got plan 04, Phase 12 has 5 plans). The ROADMAP's "Plans Complete" column is stale for these.
- **Ignoring Phase column corrections:** Some requirements were satisfied by a different phase than originally mapped (LLM-05, SEC-02, SEC-03 moved to Phase 11).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Automated sync script | Python/Bash markdown parser | Manual text edits | One-time sync; parsing markdown tables is error-prone for a one-off task |
| Validation tool | Custom counter | Manual `[x]` count or grep | grep -c '\[x\]' and grep -c 'Complete' are sufficient for validation |

**Key insight:** This is a one-time documentation correction. The effort to write a parser exceeds the effort to make ~50 text edits manually.

## Common Pitfalls

### Pitfall 1: Checkbox/Status Mismatch
**What goes wrong:** Header checkbox says `[x]` but traceability table says `Pending`, or vice versa.
**Why it happens:** Editing one location but forgetting the other.
**How to avoid:** For each requirement, always edit both the header checkbox AND the traceability table status in the same pass.
**Warning signs:** Final count of `[x]` checkboxes does not equal final count of `Complete` in traceability table.

### Pitfall 2: Miscounting Coverage Summary
**What goes wrong:** The coverage summary says "X Complete, Y Pending" but actual counts differ.
**Why it happens:** Arithmetic error after making many edits.
**How to avoid:** After all edits, count `[x]` occurrences in the header section and `Complete` occurrences in the traceability table. Both must equal the same number. Update the coverage summary last.
**Warning signs:** Numbers don't add up to 70 total.

### Pitfall 3: ROADMAP Plan Count Drift
**What goes wrong:** ROADMAP progress table shows wrong plan counts (e.g., Phase 4 says "1/3" but actually completed 3/3).
**Why it happens:** Plans were executed but the progress table was not updated in that phase's commit.
**How to avoid:** Cross-reference plan count against actual PLAN.md files and SUMMARY.md files per phase directory.
**Warning signs:** Plan count denominator does not match number of *-PLAN.md files in phase directory.

### Pitfall 4: Phase 7 Checkbox Missing
**What goes wrong:** ROADMAP shows Phase 7 as `[ ]` (incomplete) even though all 4 plans (including gap closure) are done.
**Why it happens:** Phase 7 was initially "Gap closure planned" before plan 07-04 closed the gaps. The checkbox was never updated.
**How to avoid:** Check VERIFICATION.md status -- Phase 7 is `passed` with 20/20 truths verified.

## Code Examples

Not applicable -- no code in this phase.

## Complete Delta Map

This is the authoritative reference for all changes needed. Constructed from all 12 VERIFICATION.md files.

### REQUIREMENTS.md: Header Checkboxes to Flip (from `[ ]` to `[x]`)

The following requirements currently show `[ ]` in the header section but are verified as Complete:

| Requirement | Verified By | Phase |
|-------------|-------------|-------|
| CORE-01 | Phase 3 VERIFICATION (SATISFIED) | Phase 3 |
| CORE-04 | Phase 9 VERIFICATION (Satisfied) | Phase 9 |
| CORE-05 | Phase 1 VERIFICATION (SATISFIED) | Phase 1 |
| CORE-06 | Phase 9 VERIFICATION (Satisfied -- CORE-06 listed as Phase 9 req, jemalloc from Phase 1 + bounded channels from Phase 9) | Phase 9 |
| CORE-07 | Phase 9 VERIFICATION (Satisfied -- architectural target verified by mechanism) | Phase 9 |
| CORE-08 | Phase 9 VERIFICATION (Satisfied -- architectural target verified by mechanism) | Phase 9 |
| LLM-05 | Phase 11 VERIFICATION (Satisfied -- SC-1, SC-4) | Phase 11 |
| LLM-06 | Phase 6 VERIFICATION (Satisfied -- SC-1, SC-2) | Phase 6 |
| PERS-01 | Phase 2 VERIFICATION (Satisfied) | Phase 2 |
| PERS-02 | Phase 2 VERIFICATION (Satisfied) | Phase 2 |
| PERS-03 | Phase 2 VERIFICATION (Satisfied) | Phase 2 |
| PERS-04 | Phase 2 VERIFICATION (Satisfied) | Phase 2 |
| PERS-05 | Phase 2 VERIFICATION (Satisfied) | Phase 2 |
| MEM-01 | Phase 5 VERIFICATION (Satisfied) | Phase 5 |
| MEM-02 | Phase 5 VERIFICATION (Satisfied) | Phase 5 |
| MEM-03 | Phase 5 VERIFICATION (Satisfied) | Phase 5 |
| MEM-05 | Phase 5 VERIFICATION (Satisfied) | Phase 5 |
| SEC-01 | Phase 2 VERIFICATION (Satisfied) | Phase 2 |
| SEC-02 | Phase 11 VERIFICATION (Satisfied -- SC-3) | Phase 11 |
| SEC-03 | Phase 11 VERIFICATION (Satisfied -- SC-2) | Phase 11 |
| SEC-04 | Phase 2 VERIFICATION (Satisfied) | Phase 2 |
| SEC-07 | Phase 10 VERIFICATION (VERIFIED) | Phase 10 |
| SEC-08 | Phase 2 VERIFICATION (Satisfied) | Phase 2 |
| SEC-09 | Phase 2 VERIFICATION (Satisfied) | Phase 2 |
| SEC-10 | Phase 2 VERIFICATION (Satisfied) | Phase 2 |
| COST-04 | Phase 9 VERIFICATION (Satisfied) | Phase 9 |
| PLUG-01 | Phase 8 VERIFICATION (Satisfied) | Phase 8 |
| PLUG-02 | Phase 8 VERIFICATION (Satisfied) | Phase 8 |
| PLUG-03 | Phase 8 VERIFICATION (Satisfied) | Phase 8 |
| PLUG-04 | Phase 8 VERIFICATION (Satisfied) | Phase 8 |
| CLI-01 | Phase 3 VERIFICATION (SATISFIED -- with expected secrets) | Phase 3 |
| CLI-02 | Phase 9 VERIFICATION (Satisfied) | Phase 9 |
| CLI-03 | Phase 9 VERIFICATION (Satisfied) | Phase 9 |
| CLI-04 | Phase 9 VERIFICATION (Satisfied) | Phase 9 |
| CLI-05 | Phase 3 VERIFICATION (SATISFIED) | Phase 3 |
| CLI-07 | Phase 9 VERIFICATION (Satisfied) | Phase 9 |
| CLI-08 | Phase 9 VERIFICATION (Satisfied) | Phase 9 |
| INFRA-01 | Phase 1 VERIFICATION (SATISFIED) | Phase 1 |
| INFRA-02 | Phase 1 VERIFICATION (SATISFIED) | Phase 1 |
| INFRA-03 | Phase 1 VERIFICATION (SATISFIED) | Phase 1 |
| INFRA-04 | Phase 1 VERIFICATION (SATISFIED) | Phase 1 |
| INFRA-05 | Phase 8 VERIFICATION (Satisfied) | Phase 8 |
| INFRA-06 | Phase 10 VERIFICATION (VERIFIED) | Phase 10 |

**Total: 43 checkboxes to flip from `[ ]` to `[x]`.**

Requirements already showing `[x]` (27 total): CORE-02, CORE-03, LLM-01, LLM-02, LLM-03, LLM-04, LLM-07, LLM-08, CHAN-01, CHAN-02, CHAN-03, CHAN-04, MEM-04, SEC-05, SEC-06, COST-01, COST-02, COST-03, COST-05, COST-06, SKILL-01, SKILL-02, SKILL-03, SKILL-04, SKILL-05, SKILL-06, CLI-06.

**After sync: 70/70 checkboxes will be `[x]`, 0 Pending.**

### REQUIREMENTS.md: Traceability Table Status Changes

All 43 requirements listed above need `Pending` -> `Complete` in the Status column.

### REQUIREMENTS.md: Traceability Table Phase Column Corrections

| Requirement | Current Phase | Correct Phase | Reason |
|-------------|---------------|---------------|--------|
| LLM-05 | Phase 11: Fix Critical Integration Bugs | Phase 11: Fix Critical Integration Bugs | Already correct (was updated) |
| SEC-02 | Phase 11: Fix Critical Integration Bugs | Phase 11: Fix Critical Integration Bugs | Already correct |
| SEC-03 | Phase 11: Fix Critical Integration Bugs | Phase 11: Fix Critical Integration Bugs | Already correct |
| CORE-06 | Phase 1: Project Foundation & Workspace | Phase 9: Production Hardening | Phase 1 only did jemalloc; Phase 9 completed bounded caches/channels/monitoring. Phase 1 VERIFICATION marks it PARTIAL. Phase 9 VERIFICATION marks it Satisfied. |
| SEC-01 | Phase 2: Persistence & Security Vault | Phase 2: Persistence & Security Vault | Already correct (bind_address default 127.0.0.1 in SecurityConfig) |

**Only 1 Phase column change needed:** CORE-06 from "Phase 1: Project Foundation & Workspace" to "Phase 9: Production Hardening".

Note: The Phase 2 VERIFICATION maps SEC-01 (binary binds to 127.0.0.1) to SC-4, mapping it as "Satisfied". The Phase 2 requirements list includes SEC-01. The current traceability table maps SEC-01 to Phase 2, which is correct. But checking the requirement text: SEC-01 says "Binary binds to 127.0.0.1 by default -- no open ports to the internet" -- the Phase 2 VERIFICATION SC-4 confirms `bind_address` defaulting to `"127.0.0.1"` in SecurityConfig. This is satisfied.

Also note: The SEC labels in Phase 2 VERIFICATION are: SEC-01, SEC-04, SEC-08, SEC-09, SEC-10. But the REQUIREMENTS.md traceability table has SEC-01 mapped to Phase 2, SEC-04 to Phase 2, SEC-08 to Phase 2, SEC-09 to Phase 2, SEC-10 to Phase 2. However, the actual requirement descriptions differ from the VERIFICATION mapping:
- SEC-01 = "Binary binds to 127.0.0.1 by default" -> Phase 2 VERIFICATION SC-4 confirms
- SEC-04 = "Vault key derived from passphrase via Argon2id" -> Phase 2 VERIFICATION SC-3 confirms
- SEC-08 = "Secrets redacted from all logs" -> Phase 2 VERIFICATION SC-4 confirms
- SEC-09 = "SSRF prevention enabled by default" -> Phase 2 VERIFICATION SC-4 confirms
- SEC-10 = "TLS required for all remote connections" -> Phase 2 VERIFICATION SC-4 confirms (but mapped to PERS-05 row in verification -- cross-checked: Phase 2 VERIFICATION "Requirements Coverage" table has SEC-10 -> SC-5 which is about concurrent writes. This appears to be a mapping quirk but SEC-10 is listed in the Phase 2 requirements header and TLS is verified in SC-4.)

All Phase 2 requirements are verified as Satisfied -- no changes needed to their Phase assignments.

### REQUIREMENTS.md: Coverage Summary Update

Current:
```
- v1 requirements: 70 total
- Mapped to phases: 70
- Unmapped: 0
```

Should become:
```
- v1 requirements: 70 total
- Complete: 70
- Pending: 0
```

### ROADMAP.md: Phase Checkbox Changes

| Phase | Current | Should Be | Reason |
|-------|---------|-----------|--------|
| Phase 7 | `[ ]` | `[x]` | VERIFICATION passed (20/20), all 4 plans complete |

All other phase checkboxes are already correct.

### ROADMAP.md: Progress Table Corrections

| Phase | Current Count | Correct Count | Current Status | Correct Status | Completion Date |
|-------|---------------|---------------|----------------|----------------|-----------------|
| 3. Agent Loop & Telegram | 3/3 | 4/4 | Complete | Complete | 2026-03-01 |
| 4. Context Engine & Cost Tracking | 1/3 | 3/3 | Complete | Complete | 2026-03-01 |
| 7. WASM Skill Sandbox | 3/4 | 4/4 | Gap closure planned | Complete | 2026-03-01 |
| 12. Verify Unverified Phases | 0/0 | 5/5 | Complete | Complete | 2026-03-01 |
| 13. Sync Traceability & Documentation | 0/0 | 0/0 (no plans yet) | Gap closure | Gap closure | - |

Phase 3 explanation: Plan 03-04 was added as gap closure. ROADMAP currently lists "3/3" but there are 4 plans (03-01 through 03-04), all with SUMMARYs.
Phase 4 explanation: ROADMAP says "1/3" but all 3 plans (04-01, 04-02, 04-03) have SUMMARYs. The 1/3 is stale from when only the first plan was complete.
Phase 7 explanation: Plan 07-04 was added as gap closure. All 4 plans have SUMMARYs.
Phase 12 explanation: 5 plans were created and executed (12-01 through 12-05), all with SUMMARYs. ROADMAP says "0/0".

### ROADMAP.md: Plan Listing Corrections

Within the Phase Details section, some plan listings need checkbox updates:

**Phase 3:** Plan `03-04-PLAN.md` currently shows `[ ]` -- should be `[x]` (has SUMMARY, verified in Phase 3 re-verification)

**Phase 4:** Plans `04-01-PLAN.md`, `04-02-PLAN.md`, `04-03-PLAN.md` currently show `[ ]` -- should all be `[x]` (all have SUMMARYs, verified in Phase 4 VERIFICATION)

**Phase 7:** Plan `07-04-PLAN.md` currently shows `[ ]` -- should be `[x]` (has SUMMARY, verified in Phase 7 re-verification)

**Phase 10:** Plan listing currently shows placeholder `10-01: TBD` with `[ ]`. Should be updated to reflect the actual 3 plans: 10-01, 10-02, 10-03 (all with SUMMARYs). But 10-01 shows `[ ]` in the listing area -- needs correction.

**Phase 12:** Plan listing is empty. Should list 12-01 through 12-05 with `[x]` checkboxes.

**Phase 13:** Plan listing is empty. This is correct since Phase 13 has no plans yet.

### ROADMAP.md: Execution Order Section

The "Execution Order" text mentions "Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5 -> 6 -> 7 -> 8 -> 9 -> 10" but doesn't include phases 11, 12, 13. This could optionally be updated for completeness, but is not critical.

## Detailed Edit Sequence

### Recommended Edit Order

1. **REQUIREMENTS.md traceability table** -- Flip all 43 `Pending` -> `Complete`, fix CORE-06 Phase column
2. **REQUIREMENTS.md header checkboxes** -- Flip all 43 `[ ]` -> `[x]`
3. **REQUIREMENTS.md coverage summary** -- Update counts to 70 Complete, 0 Pending
4. **ROADMAP.md Phase 7 checkbox** -- Flip `[ ]` -> `[x]`
5. **ROADMAP.md progress table** -- Fix plan counts and statuses for Phase 3, 4, 7, 12
6. **ROADMAP.md plan listings** -- Fix checkboxes for plans 03-04, 04-01, 04-02, 04-03, 07-04, 10-01/02/03, 12-01 through 12-05
7. **Validation pass** -- Count [x] checkboxes = 70, count Complete statuses = 70, verify sum = 70

## Open Questions

1. **ROADMAP Phase 10 plan listing stale**
   - What we know: Phase 10 has 3 plans (10-01, 10-02, 10-03) all completed. ROADMAP shows only "10-01: TBD" placeholder.
   - What's unclear: Whether to add the full plan descriptions (matching the format used for other phases) or leave as-is since Phase 10 is complete.
   - Recommendation: Add plan descriptions for 10-01, 10-02, 10-03 with `[x]` checkboxes to match the format of all other phases. The SUMMARY files provide the needed descriptions.

2. **SEC-02 requirement description ambiguity**
   - What we know: SEC-02 says "Device keypair authentication required -- no optional auth mode". The REQUIREMENTS.md header maps this to Phase 2, but the traceability table already maps it to Phase 11. Phase 11 wired keypair auth into gateway. Phase 9 VERIFICATION also references SEC-02 (fail-closed enforcement).
   - What's unclear: Whether the traceability table Phase should say Phase 9 or Phase 11.
   - Recommendation: Keep Phase 11 as shown in current traceability table -- Phase 11 is where SEC-02 was explicitly listed as a requirement and verified.

## Sources

### Primary (HIGH confidence)
- `.planning/phases/01-project-foundation-workspace/01-VERIFICATION.md` -- Phase 1 requirements coverage
- `.planning/phases/02-persistence-security-vault/02-VERIFICATION.md` -- Phase 2 requirements coverage (10 reqs)
- `.planning/phases/03-agent-loop-telegram/03-VERIFICATION.md` -- Phase 3 requirements coverage (12 reqs)
- `.planning/phases/04-context-engine-cost-tracking/04-VERIFICATION.md` -- Phase 4 requirements coverage (9 reqs)
- `.planning/phases/05-memory-embeddings/05-VERIFICATION.md` -- Phase 5 requirements coverage (4 reqs)
- `.planning/phases/06-model-routing-smart-heartbeats/06-VERIFICATION.md` -- Phase 6 requirements coverage (1 req: LLM-06)
- `.planning/phases/07-wasm-skill-sandbox/07-VERIFICATION.md` -- Phase 7 requirements coverage (8 reqs)
- `.planning/phases/08-plugin-system-gateway/08-VERIFICATION.md` -- Phase 8 requirements coverage (5 reqs)
- `.planning/phases/09-production-hardening/09-VERIFICATION.md` -- Phase 9 requirements coverage (10 reqs)
- `.planning/phases/10-multi-agent-final-integration/10-VERIFICATION.md` -- Phase 10 requirements coverage (2 reqs)
- `.planning/phases/11-fix-integration-bugs/11-VERIFICATION.md` -- Phase 11 requirements coverage (3 reqs)
- `.planning/phases/12-verify-unverified-phases/12-VERIFICATION.md` -- Phase 12 meta-verification (30 reqs)
- `.planning/REQUIREMENTS.md` -- Current traceability table (source of drift)
- `.planning/ROADMAP.md` -- Current progress table (source of drift)

### Secondary (MEDIUM confidence)
- `.planning/STATE.md` -- Project state and progress metrics
- All `*-SUMMARY.md` files across all phases (39 total) -- Confirmation of plan completion

## Metadata

**Confidence breakdown:**
- Delta map accuracy: HIGH -- Mechanically derived from VERIFICATION.md files, cross-referenced against REQUIREMENTS.md
- Edit sequence: HIGH -- Straightforward text replacements with no ambiguity
- Coverage count: HIGH -- All 70 requirements verified as Complete across 12 phases

**Research date:** 2026-03-02
**Valid until:** Indefinite -- one-time sync, findings do not expire
