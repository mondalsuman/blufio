# Phase 1: Project Foundation & Workspace - Context

**Gathered:** 2026-02-28
**Status:** Ready for planning

<domain>
## Phase Boundary

Cargo workspace with core trait definitions, TOML config system with strict validation, CI pipeline with license and vulnerability auditing, and dual licensing. The project builds, tests, and enforces quality gates from the first commit. No runtime features — this is pure foundation.

</domain>

<decisions>
## Implementation Decisions

### Workspace layout
- Domain crates under `crates/` directory — root Cargo.toml is workspace-only
- Phase 1 creates 3 crates: `blufio-core` (traits, error types, common types), `blufio-config` (TOML parsing, validation), `blufio` (binary crate, CLI entry point)
- Additional crates (blufio-storage, blufio-channel, blufio-agent, etc.) added by later phases as needed
- Stub trait signatures for all 7 adapter traits (Channel, Provider, Storage, Embedding, Observability, Auth, SkillRuntime) defined in blufio-core with empty/todo!() default impls — establishes architecture contract early

### Config experience
- Config file named `blufio.toml` with XDG lookup hierarchy: `./blufio.toml` → `~/.config/blufio/blufio.toml` → `/etc/blufio/blufio.toml`
- Flat section organization: `[agent]`, `[telegram]`, `[anthropic]`, `[storage]`, `[security]`, `[cost]` — one level of nesting
- Actionable error messages with line numbers, typo suggestions (fuzzy matching), and valid key listings on invalid config
- Environment variable overrides with `BLUFIO_` prefix — `BLUFIO_TELEGRAM_BOT_TOKEN` overrides `telegram.bot_token` in TOML
- `deny_unknown_fields` on all config structs (requirement CLI-06)

### CI & quality gates
- GitHub Actions as CI platform
- Four merge-blocking checks: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo deny check`, `cargo audit`
- musl cross-compilation runs on release tags only (not every PR) — faster CI feedback loop
- Latest stable Rust targeted — no MSRV guarantee
- CI matrix: stable Rust on Linux + macOS

### Project identity
- Code of conduct: Contributor Covenant v2.1
- Governance: BDFL (Benevolent Dictator For Life) model
- CONTRIBUTING.md tone: Direct and technical — build/test/submit instructions, style guide, tests required, no fluff
- Security disclosure: GitHub private security advisories — 90-day disclosure timeline, acknowledge within 48h
- Community docs from day one: CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md, GOVERNANCE.md (requirement INFRA-04)

### Claude's Discretion
- Exact crate dependency versions
- Internal module structure within each crate
- GitHub Actions workflow file structure (single vs multiple workflow files)
- Specific cargo-deny.toml configuration beyond license checks
- SPDX header format and automation
- README.md content and structure

</decisions>

<specifics>
## Specific Ideas

- Architecture philosophy is "everything is a plugin" — the 7 adapter traits in blufio-core are the contract that all later phases implement against
- Config errors should feel like Elm compiler errors — helpful, not cryptic
- The project is positioned as a "kill shot" against OpenClaw — quality signals (licensing, CI, security docs) matter from commit one

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- None — greenfield project, no existing code

### Established Patterns
- None yet — Phase 1 establishes all patterns

### Integration Points
- PRD documents in `/PRD/` directory contain detailed architecture specs that inform trait design
- `.planning/REQUIREMENTS.md` maps specific requirements (CORE-05, CORE-06, CLI-06, INFRA-01-04) to this phase

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 01-project-foundation-workspace*
*Context gathered: 2026-02-28*
