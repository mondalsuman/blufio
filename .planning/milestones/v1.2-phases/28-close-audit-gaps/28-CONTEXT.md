# Phase 28: Close Audit Gaps - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Close all gaps identified in v1.2-MILESTONE-AUDIT.md. Fix CIPH-01 feature flag, create missing verification files for phases 25 and 27, update traceability across REQUIREMENTS.md and SUMMARY frontmatter. This is a documentation/process gap closure phase — all requirements are functionally wired (29/30), only bookkeeping and one feature flag name need fixing.

</domain>

<decisions>
## Implementation Decisions

### Tech debt scope
- Required gap closures ONLY — the 5 actions specified by the audit
- Low-severity tech debt items (encrypt.rs duplication, optional dependency hygiene) are NOT in scope
- Those can be addressed in a future maintenance phase if needed

### Verification depth
- Cross-reference existing audit evidence (integration checker results) as the basis
- Verify each requirement against what the audit already confirmed is wired
- No need for independent re-inspection — audit already confirmed 29/30 requirements are functionally wired

### CIPH-01 fix validation
- Change `bundled-sqlcipher` to `bundled-sqlcipher-vendored-openssl` in workspace Cargo.toml
- Run `cargo check` after the change to validate it compiles
- If build fails, investigate and fix before proceeding

### Claude's Discretion
- Verification file format and structure (follow existing patterns from 23-VERIFICATION.md, 24-VERIFICATION.md, 26-VERIFICATION.md)
- Order of gap closure operations
- Exact wording in REQUIREMENTS.md checkbox updates

</decisions>

<specifics>
## Specific Ideas

No specific requirements — follow the 5 closure actions exactly as specified in v1.2-MILESTONE-AUDIT.md:
1. Fix CIPH-01 feature flag in Cargo.toml
2. Create 25-VERIFICATION.md verifying CIPH-01..08
3. Create 27-VERIFICATION.md verifying UPDT-01..08
4. Update all 30 v1.2 requirements to `[x]` Complete in REQUIREMENTS.md
5. Populate requirements_completed in SUMMARY frontmatter for 26-01, 26-02, 27-01, 27-02

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- Existing verification files (23-VERIFICATION.md, 24-VERIFICATION.md, 26-VERIFICATION.md) as templates for format
- v1.2-MILESTONE-AUDIT.md contains all evidence needed for verification cross-references
- REQUIREMENTS.md has the traceability table structure already in place

### Established Patterns
- Verification files follow a per-requirement PASS/FAIL format with evidence
- SUMMARY frontmatter uses `requirements_completed: [REQ-ID, ...]` array format
- REQUIREMENTS.md uses `[x]` checkboxes with status column in traceability table

### Integration Points
- Cargo.toml line 29: `rusqlite = { version = "0.37", features = ["bundled-sqlcipher"] }` — change target
- .planning/phases/25-sqlcipher-database-encryption/ — verification file destination
- .planning/phases/27-self-update-with-rollback/ — verification file destination
- .planning/REQUIREMENTS.md — checkbox and traceability table updates
- .planning/phases/26-*/26-01-SUMMARY.md, 26-02-SUMMARY.md — frontmatter updates (SIGN-01..04)
- .planning/phases/27-*/27-01-SUMMARY.md, 27-02-SUMMARY.md — frontmatter updates (UPDT-01..08)

</code_context>

<deferred>
## Deferred Ideas

- encrypt.rs integrity check duplication fix (low severity) — future maintenance
- Optional dependency hygiene for blufio-storage (low severity) — future maintenance

</deferred>

---

*Phase: 28-close-audit-gaps*
*Context gathered: 2026-03-04*
