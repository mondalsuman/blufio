# Phase 1: Project Foundation & Workspace - Research

**Researched:** 2026-02-28
**Domain:** Rust project infrastructure -- Cargo workspace, config parsing, CI/CD, licensing, trait architecture
**Confidence:** HIGH

## Summary

Phase 1 establishes the entire build/test/quality infrastructure for Blufio from the first commit. The Rust ecosystem has well-established patterns for every requirement: Cargo workspace inheritance for multi-crate projects, figment for layered configuration with provenance-tracked error messages, tikv-jemallocator for jemalloc integration, and EmbarkStudios cargo-deny for license auditing. The critical architectural decision is defining 7 stub adapter traits in blufio-core using `#[async_trait]` (dtolnay) for dyn-dispatch compatibility, since native `async fn in trait` (stable since Rust 1.75) still cannot produce trait objects -- a hard requirement for the plugin architecture.

The biggest pitfall is the `#[serde(deny_unknown_fields)]` + `#[serde(flatten)]` incompatibility in serde. Since the config system needs `deny_unknown_fields` (requirement CLI-06) and may eventually need flattened structs, config structs must be designed flat from the start -- no `#[serde(flatten)]` anywhere. Figment handles the TOML + env var merging layer, serde handles the validation layer, and miette provides Elm-style diagnostic rendering for config errors.

**Primary recommendation:** Use figment 0.10 with `Env::map()` (NOT `split("_")`) for config merging (TOML + env + defaults), serde with `deny_unknown_fields` on all config structs (no flatten), a custom Figment-to-miette bridge for Elm-style error display leveraging `Kind::UnknownField`'s built-in valid field list, `#[async_trait]` for all adapter trait definitions with concrete `BlufioError` return types, and virtual Cargo workspace manifest with `workspace.dependencies` inheritance. blufio-core should have zero tokio dependency -- `async_trait` only needs std types.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Workspace layout: crates/ directory with blufio-core, blufio-config, blufio (binary) -- root Cargo.toml is workspace-only (virtual manifest)
- Config file named `blufio.toml` with XDG lookup hierarchy: `./blufio.toml` -> `~/.config/blufio/blufio.toml` -> `/etc/blufio/blufio.toml`
- Flat section organization: `[agent]`, `[telegram]`, `[anthropic]`, `[storage]`, `[security]`, `[cost]` -- one level of nesting
- Actionable error messages with line numbers, typo suggestions (fuzzy matching), and valid key listings on invalid config
- Environment variable overrides with `BLUFIO_` prefix -- `BLUFIO_TELEGRAM_BOT_TOKEN` overrides `telegram.bot_token` in TOML
- `deny_unknown_fields` on all config structs (requirement CLI-06)
- GitHub Actions CI with four merge-blocking checks: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo deny check`, `cargo audit`
- musl cross-compilation runs on release tags only (not every PR)
- Latest stable Rust targeted -- no MSRV guarantee
- CI matrix: stable Rust on Linux + macOS
- Stub trait signatures for all 7 adapter traits (Channel, Provider, Storage, Embedding, Observability, Auth, SkillRuntime) in blufio-core
- Code of conduct: Contributor Covenant v2.1
- Governance: BDFL model
- CONTRIBUTING.md tone: Direct and technical
- Security disclosure: GitHub private security advisories -- 90-day disclosure timeline, acknowledge within 48h

### Claude's Discretion
- Exact crate dependency versions
- Internal module structure within each crate
- GitHub Actions workflow file structure (single vs multiple workflow files)
- Specific cargo-deny.toml configuration beyond license checks
- SPDX header format and automation
- README.md content and structure

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CORE-05 | Binary ships as single static executable (~25MB core) with musl static linking | musl cross-compilation via cross-rs or native target, release profile with LTO, tikv-jemallocator compatibility verified |
| CORE-06 | Process uses jemalloc allocator with bounded LRU caches, bounded channels (backpressure), and lock timeouts | tikv-jemallocator 0.6 `#[global_allocator]` pattern; bounded caches/channels are later-phase concerns, jemalloc wiring is Phase 1 |
| CLI-06 | TOML config with deny_unknown_fields catches typos at startup | figment TOML+Env merging, serde `deny_unknown_fields`, miette diagnostic rendering, strsim fuzzy matching |
| INFRA-01 | Dual-license MIT + Apache-2.0 from first commit with SPDX headers | SPDX-FileCopyrightText + SPDX-License-Identifier headers, LICENSE-MIT and LICENSE-APACHE files, Cargo.toml `license = "MIT OR Apache-2.0"` |
| INFRA-02 | cargo-deny.toml enforces license compatibility in CI | EmbarkStudios cargo-deny with allow-list for MIT, Apache-2.0, BSD-3-Clause, ISC, Unicode-3.0, Zlib |
| INFRA-03 | cargo-audit runs in CI for vulnerability scanning | actions-rust-lang/audit@v1 in GitHub Actions with daily schedule + PR trigger |
| INFRA-04 | CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md, GOVERNANCE.md from day one | Templates from Contributor Covenant v2.1, BDFL governance doc, GitHub security advisories |
</phase_requirements>

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| figment | 0.10.19 | Layered config merging (TOML + env vars + defaults) | Provenance tracking for error messages, Env::prefixed("BLUFIO_").split("_") for nested keys, 15.9M downloads |
| serde | 1.x | Serialization/deserialization with `deny_unknown_fields` | Universal Rust serialization standard, derive macros for config structs |
| toml | 0.8.x | TOML format provider for figment | Standard Rust TOML parser, serde-compatible |
| tikv-jemallocator | 0.6.1 | jemalloc global allocator | TiKV-maintained fork, `#[global_allocator]` integration, Tier 1 Linux/macOS support |
| tikv-jemalloc-ctl | 0.6.x | jemalloc introspection (stats, epoch) | Companion to tikv-jemallocator for runtime memory stats |
| thiserror | 2.0.x | Derive macro for error types | Standard Rust error type derivation, `From` impl generation |
| miette | 7.6.x | Diagnostic error rendering (Elm-style) | Fancy ANSI/Unicode error display with source spans, labels, and suggestions |
| clap | 4.5.x | CLI argument parsing with derive API | De facto Rust CLI parser, subcommand support, shell completions |
| async-trait | 0.1.x | Async methods in trait objects (dyn dispatch) | Required because native `async fn in trait` is not dyn-compatible (as of Rust 1.85) |
| strum | 0.26.x | Enum derive macros (Display, EnumString, AsRefStr) | Adapter type enum display, iteration |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| dirs | 6.x | XDG/platform-specific directory paths | Config file XDG lookup (`config_dir()` for `~/.config/blufio/`) |
| strsim | 0.11.x | String similarity (Levenshtein, Jaro-Winkler) | Fuzzy matching for typo suggestions in config error messages |
| semver | 1.x | Semantic versioning types | Plugin adapter version fields |
| tracing | 0.1.x | Structured logging framework | Error context logging throughout (wired in Phase 1, used extensively later) |
| tokio | 1.x | Async runtime | Required by async-trait adapter stubs; workspace-level dependency |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| figment | config-rs | config-rs is more mature but lacks provenance tracking; figment error messages point to exact source file/env var that caused the error -- critical for Elm-style config errors |
| miette | ariadne | ariadne is lower-level (requires building your own diagnostic types); miette provides derive macros and integrates with thiserror |
| async-trait (dtolnay) | trait-variant | trait-variant creates Send/non-Send pairs but does NOT support dyn dispatch -- a hard requirement for the plugin host architecture |
| dirs | directories | directories provides ProjectDirs with app-specific paths, but we need simple XDG lookups; dirs is lower-level and simpler |
| cross-rs | native musl target | For CI: `cross` uses Docker containers and works on any host; native musl target requires musl-tools installed on the runner. For release-only builds, cross-rs is simpler. |

**Cargo.toml workspace dependencies (root):**
```toml
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
toml = "0.8"
figment = { version = "0.10", features = ["toml", "env"] }
thiserror = "2"
miette = { version = "7", features = ["fancy"] }
clap = { version = "4.5", features = ["derive"] }
async-trait = "0.1"
strum = { version = "0.26", features = ["derive"] }
dirs = "6"
strsim = "0.11"
semver = "1"
tracing = "0.1"
tokio = { version = "1" }               # NO features at workspace level (see Deep Dive: Tokio)
tikv-jemallocator = "0.6"
tikv-jemalloc-ctl = "0.6"
```

## Architecture Patterns

### Recommended Project Structure
```
blufio/
├── Cargo.toml                    # Virtual manifest (workspace only, no [package])
├── Cargo.lock                    # Shared lockfile
├── deny.toml                     # cargo-deny configuration
├── rust-toolchain.toml           # Pin stable toolchain
├── .github/
│   └── workflows/
│       ├── ci.yml                # fmt + clippy + test + deny (every PR)
│       └── audit.yml             # cargo-audit (daily schedule + PR)
│       └── release.yml           # musl cross-compile (release tags only)
├── LICENSE-MIT
├── LICENSE-APACHE
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── SECURITY.md
├── GOVERNANCE.md
├── README.md
└── crates/
    ├── blufio-core/
    │   ├── Cargo.toml            # Traits, error types, common types
    │   └── src/
    │       ├── lib.rs
    │       ├── error.rs          # BlufioError enum (thiserror)
    │       ├── types.rs          # Common types (SessionKey, MessageId, etc.)
    │       └── traits/
    │           ├── mod.rs
    │           ├── adapter.rs    # PluginAdapter base trait
    │           ├── channel.rs    # ChannelAdapter trait
    │           ├── provider.rs   # ProviderAdapter trait
    │           ├── storage.rs    # StorageAdapter trait
    │           ├── embedding.rs  # EmbeddingAdapter trait
    │           ├── observability.rs # ObservabilityAdapter trait
    │           ├── auth.rs       # AuthAdapter trait
    │           └── skill.rs      # SkillRuntime trait
    ├── blufio-config/
    │   ├── Cargo.toml            # Config parsing, validation, error display
    │   └── src/
    │       ├── lib.rs
    │       ├── model.rs          # Config structs with serde + deny_unknown_fields
    │       ├── loader.rs         # Figment assembly (TOML + env + defaults)
    │       ├── validation.rs     # Post-deserialization validation
    │       └── diagnostic.rs     # Miette error formatting, fuzzy match suggestions
    └── blufio/
        ├── Cargo.toml            # Binary crate
        └── src/
            └── main.rs           # Entry point, jemalloc global_allocator, clap CLI
```

### Pattern 1: Virtual Workspace with Dependency Inheritance

**What:** Root Cargo.toml is a virtual manifest (no `[package]` section). All shared dependencies declared once in `[workspace.dependencies]`, inherited by members via `dependency.workspace = true`.

**When to use:** Always for multi-crate workspaces. Prevents version drift across crates.

**Example (root Cargo.toml):**
```toml
# Source: https://doc.rust-lang.org/cargo/reference/workspaces.html

[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
license = "MIT OR Apache-2.0"
repository = "https://github.com/your-org/blufio"
authors = ["Blufio Contributors"]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
thiserror = "2"
# ... all shared deps here

[profile.release]
lto = "thin"
codegen-units = 1
strip = "debuginfo"

[profile.release-musl]
inherits = "release"
lto = true
opt-level = "s"
panic = "abort"
```

**Example (member crates/blufio-core/Cargo.toml):**
```toml
[package]
name = "blufio-core"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true

[dependencies]
serde.workspace = true
thiserror.workspace = true
async-trait.workspace = true
semver.workspace = true
strum.workspace = true
tokio.workspace = true
```

### Pattern 2: Jemalloc Global Allocator (Binary Crate Only)

**What:** Set jemalloc as the global allocator in the binary crate's `main.rs`. Only the final binary crate touches allocator selection -- library crates are allocator-agnostic.

**When to use:** Always in `crates/blufio/src/main.rs`. Never in library crates.

**Example:**
```rust
// Source: https://github.com/tikv/jemallocator README
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() {
    // jemalloc is now the global allocator
    // All heap allocations across all crates use jemalloc
}
```

### Pattern 3: Figment Layered Config with XDG Lookup

**What:** Assemble configuration from multiple sources with clear priority: defaults < /etc/blufio/ < ~/.config/blufio/ < ./blufio.toml < env vars. Figment tracks which source provided each value.

**When to use:** In `blufio-config` crate's loader module.

**Example:**
```rust
// Source: https://github.com/SergioBenitez/Figment README + docs.rs/figment
use figment::{Figment, providers::{Format, Toml, Env, Serialized}};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentConfig {
    name: String,
    #[serde(default = "default_max_sessions")]
    max_sessions: usize,
}

fn default_max_sessions() -> usize { 10 }

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BlufioConfig {
    agent: AgentConfig,
    // Each section is a separate struct with deny_unknown_fields
}

pub fn load_config() -> Result<BlufioConfig, figment::Error> {
    Figment::new()
        // Layer 1: Compiled defaults
        .merge(Serialized::defaults(BlufioConfig::default()))
        // Layer 2: System-wide config
        .merge(Toml::file("/etc/blufio/blufio.toml"))
        // Layer 3: User config (XDG)
        .merge(Toml::file(
            dirs::config_dir()
                .map(|d| d.join("blufio/blufio.toml"))
                .unwrap_or_default()
        ))
        // Layer 4: Local config (current directory)
        .merge(Toml::file("blufio.toml"))
        // Layer 5: Environment variable overrides
        // NOTE: Do NOT use .split("_") -- see Deep Dive: Figment Env Mapping
        // Use explicit .map() to avoid underscore ambiguity in key names
        .merge(env_provider())
        .extract()
}
```

### Pattern 4: Adapter Trait with async-trait for Dyn Dispatch

**What:** Define adapter traits using `#[async_trait]` macro so they can be used as trait objects (`Box<dyn ChannelAdapter>`). Native async fn in trait (Rust 1.75+) does NOT support dyn dispatch.

**When to use:** All 7 adapter traits in blufio-core.

**Example:**
```rust
// Source: https://docs.rs/async-trait + PRD Section 2.2
use async_trait::async_trait;

/// Base trait for all plugin adapters
#[async_trait]
pub trait PluginAdapter: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn version(&self) -> semver::Version;
    fn adapter_type(&self) -> AdapterType;
    async fn health_check(&self) -> Result<HealthStatus, crate::error::BlufioError>;
    async fn shutdown(&self) -> Result<(), crate::error::BlufioError>;
}

/// Channel adapters handle messaging I/O
#[async_trait]
pub trait ChannelAdapter: PluginAdapter {
    async fn connect(&mut self, config: &ChannelConfig) -> Result<(), crate::error::BlufioError>;
    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, crate::error::BlufioError>;
    async fn receive(&self) -> Result<InboundMessage, crate::error::BlufioError>;
    // ... stub methods with todo!() or unimplemented!()
}
```

### Pattern 5: SPDX Dual-License Headers

**What:** Every `.rs` source file begins with SPDX copyright and license identifier comments. Two license files at repo root.

**When to use:** Every source file, from the first commit.

**Example:**
```rust
// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
```

### Anti-Patterns to Avoid
- **Root package in workspace manifest:** Do NOT put a `[package]` section in the root Cargo.toml. Use a virtual manifest. Putting the binary crate at root pollutes the workspace root with `src/`, requires `--workspace` flags, and breaks the consistent `crates/` structure.
- **Stripping crate prefix from folder names:** Keep folder names matching crate names (`blufio-core/`, not `core/`). Navigation and rename operations become ambiguous without the prefix.
- **Using `#[serde(flatten)]` with `deny_unknown_fields`:** These are incompatible in serde. Neither the outer nor inner struct can use both. Design config structs flat from day one.
- **Native async fn in trait for plugin traits:** Rust 1.75+ supports `async fn` in traits, but these are NOT dyn-compatible. Plugin traits MUST use `#[async_trait]` for `Box<dyn Trait>` support.
- **Jemalloc in library crates:** `#[global_allocator]` belongs ONLY in the final binary crate. Library crates must be allocator-agnostic.
- **Version = "0.0.0" for publishable crates:** Use `version = "0.1.0"` and `publish = false` for internal crates. `0.0.0` can cause issues with some cargo tooling.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Config file merging with env var overrides | Custom TOML parser + env var merger | figment with Toml + Env providers | Provenance tracking, error source attribution, battle-tested merge semantics (merge vs join) |
| Config error formatting with suggestions | Custom error pretty-printer | miette + strsim | miette handles ANSI rendering, source spans, labels; strsim provides edit-distance for typo suggestions |
| License compliance checking | grep-based license scanner | cargo-deny (EmbarkStudios) | Handles SPDX expression parsing, license file inference (0.93 confidence threshold), dual-license resolution, transitive dependency scanning |
| Vulnerability scanning | Manual CVE tracking | cargo-audit + RustSec Advisory DB | Automated, maintained by RustSec org, GitHub Advisory DB integration, issue creation |
| Async trait dyn dispatch | Manual Pin<Box<dyn Future>> wrapping | async-trait (dtolnay) | Handles lifetime elision, Send bounds, Pin/Box transformation correctly -- extremely error-prone to do manually |
| CI caching for Rust | Manual cache key construction | Swatinem/rust-cache@v2 | Smart cache invalidation based on rustc version, Cargo.lock hash, workspace structure |
| Cross-compilation | Manual musl toolchain setup | cross-rs/cross or houseabsolute/actions-rust-cross | Docker-based compilation with correct musl libc, OpenSSL, and system library versions |
| SPDX header management | Regex-based header insertion scripts | reuse-tool (FSFE) | REUSE 3.3 spec compliant, handles edge cases (binary files, .toml files), lint mode for CI verification |

**Key insight:** Config error formatting, license compliance, and cross-compilation are all deceptively complex problems where edge cases dominate. Every "simple" hand-rolled solution eventually needs to handle the same edge cases these established tools already handle.

## Common Pitfalls

### Pitfall 1: deny_unknown_fields + flatten Incompatibility
**What goes wrong:** Using `#[serde(flatten)]` on any struct that also has `#[serde(deny_unknown_fields)]` causes deserialization to fail even when all fields are valid.
**Why it happens:** Serde's flatten implementation passes unknown fields to the flattened struct, but deny_unknown_fields on the outer struct rejects them before they reach the inner struct. This is a known, long-standing serde limitation (issue #1600).
**How to avoid:** Never combine `flatten` and `deny_unknown_fields`. Design all config structs with explicit nested sections from day one. Use Figment's section-based merging instead of serde flatten.
**Warning signs:** Deserialization errors mentioning "unknown field" for fields that clearly exist in nested structs.

### Pitfall 2: Jemalloc + musl Profiling Segfault
**What goes wrong:** Enabling tikv-jemallocator's `profiling` feature on `x86_64-unknown-linux-musl` target causes immediate segfault.
**Why it happens:** jemalloc profiling relies on `dladdr` which behaves differently under musl libc compared to glibc. (GitHub issue tikv/jemallocator#146)
**How to avoid:** Do NOT enable the `profiling` feature for musl builds. If heap profiling is needed, use it only on glibc targets (macOS, Linux-gnu). For Phase 1, only the default `background_threads_runtime_support` feature is needed.
**Warning signs:** Segfault on startup of musl binary, works fine on macOS/Linux-gnu.

### Pitfall 3: Workspace Dependency Feature Additivity
**What goes wrong:** Features declared in `[workspace.dependencies]` are additive with features in member `[dependencies]`. If the workspace declares `tokio = { features = ["full"] }`, you cannot have a member crate that uses tokio without the "full" feature set.
**Why it happens:** Cargo feature unification means all features are merged. Workspace-level features become the minimum feature set.
**How to avoid:** Declare only the minimal feature set in `[workspace.dependencies]`. Let individual crates add features they need. For tokio, declare without features at workspace level; add `features = ["full"]` only in the binary crate.
**Warning signs:** Unexpectedly large compile times for library crates that should only need a subset of a dependency's features.

### Pitfall 4: CI Cache Invalidation with Clippy
**What goes wrong:** Clippy artifacts and cargo build artifacts are similar but not identical. Sharing a single cache between `cargo build` and `cargo clippy` causes cache thrashing.
**Why it happens:** Clippy modifies compilation flags, producing different artifacts. Each job overwrites the other's cache.
**How to avoid:** Use separate cache keys for build and clippy jobs, or run clippy first (it includes build), or use `Swatinem/rust-cache@v2` with separate `shared-key` values per job.
**Warning signs:** CI builds are slower than expected, cache size grows unbounded, "recompiling" messages in CI logs for dependencies that should be cached.

### Pitfall 5: Figment Env Split Ambiguity (CRITICAL -- updated by deep dive)
**What goes wrong:** `Env::prefixed("BLUFIO_").split("_")` is fundamentally broken for config keys containing underscores. `BLUFIO_TELEGRAM_BOT_TOKEN` maps to `telegram.bot.token` instead of `telegram.bot_token`. The Figment maintainer confirms: "there does not exist an unambiguous way to split an environment variable name into nestings."
**Why it happens:** `A_B_C` could mean `A[B][C]`, `A_B[C]`, or `A[B_C]` -- 2^n possible interpretations for n underscores.
**How to avoid:** Use `Env::map()` with explicit section-to-dot mapping instead of `split()`. See Deep Dive: Figment Environment Variable Mapping for the complete pattern.
**Warning signs:** Config values appearing under wrong keys when set via environment variables. Bot tokens splitting into nested dictionaries.

### Pitfall 6: musl Default Allocator 9x Slowdown (added by deep dive)
**What goes wrong:** musl builds without jemalloc are ~9x slower than glibc builds under multi-threaded workloads (513ms vs 56ms in benchmarks).
**Why it happens:** musl's default allocator uses a single global lock for all allocations, causing extreme lock contention under concurrent allocation patterns.
**How to avoid:** ALWAYS use tikv-jemallocator as `#[global_allocator]` in the binary crate. This is not optional for musl targets. Use `#[cfg(not(target_env = "msvc"))]` conditional compilation.
**Warning signs:** Inexplicable performance regression when switching from `linux-gnu` to `linux-musl` target. All benchmarks suddenly 5-10x slower.

### Pitfall 7: Missing `resolver = "2"` in Workspace
**What goes wrong:** Without `resolver = "2"`, Cargo uses the v1 feature resolver which unifies features across all workspace members, including dev-dependencies and build-dependencies.
**Why it happens:** The v1 resolver was the default before Rust 2021 edition. Virtual manifests don't have an edition field, so the resolver must be explicitly set.
**How to avoid:** Always include `resolver = "2"` in the workspace `[workspace]` section. The 2024 edition (used with `edition = "2024"`) implies resolver v2 for packages but NOT for virtual manifests.
**Warning signs:** Features intended only for dev-dependencies bleeding into release builds.

## Code Examples

### Complete cargo-deny.toml Configuration
```toml
# Source: https://embarkstudios.github.io/cargo-deny/

[graph]
targets = [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
]
all-features = true

[advisories]
# Treat unmaintained crates as warnings (not errors) initially
unmaintained = "warn"
ignore = []

[bans]
multiple-versions = "warn"
wildcards = "deny"
deny = [
    { crate = "openssl", use-instead = "rustls" },
    { crate = "openssl-sys", use-instead = "rustls" },
]

[sources]
unknown-registry = "deny"
unknown-git = "deny"

[licenses]
confidence-threshold = 0.93
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Zlib",
    "Unicode-DFS-2016",
]

[[licenses.clarify]]
name = "ring"
expression = "MIT AND ISC AND OpenSSL"
license-files = [
    { path = "LICENSE", hash = 0xbd0eed23 },
]
```

### GitHub Actions CI Workflow (ci.yml)
```yaml
# Source: Composite from dtolnay/rust-toolchain, Swatinem/rust-cache, EmbarkStudios/cargo-deny-action

name: CI
on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always

jobs:
  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all --check

  clippy:
    name: Clippy
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: clippy-${{ matrix.os }}
      - run: cargo clippy --workspace --all-targets -- -D warnings

  test:
    name: Test
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: test-${{ matrix.os }}
      - run: cargo test --workspace

  deny:
    name: Deny
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check
          arguments: --all-features
```

### GitHub Actions Audit Workflow (audit.yml)
```yaml
# Source: https://github.com/actions-rust-lang/audit

name: Audit
on:
  push:
    paths:
      - '.github/workflows/audit.yml'
      - '**/Cargo.toml'
      - '**/Cargo.lock'
  schedule:
    - cron: '0 0 * * *'
  workflow_dispatch:

permissions:
  contents: read
  issues: write

jobs:
  audit:
    name: Audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/audit@v1
        name: Audit Rust Dependencies
```

### GitHub Actions Release Workflow (release.yml)
```yaml
# Source: Composite from cross-rs/cross, houseabsolute/actions-rust-cross

name: Release
on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
          - target: aarch64-unknown-linux-musl
            os: ubuntu-latest
          - target: x86_64-apple-darwin
            os: macos-latest
          - target: aarch64-apple-darwin
            os: macos-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install cross (Linux musl only)
        if: contains(matrix.target, 'musl')
        run: cargo install cross --git https://github.com/cross-rs/cross
      - name: Build (cross for musl)
        if: contains(matrix.target, 'musl')
        run: cross build --release --target ${{ matrix.target }} -p blufio
      - name: Build (native for macOS)
        if: "!contains(matrix.target, 'musl')"
        run: cargo build --release --target ${{ matrix.target }} -p blufio
```

### Config Error Diagnostic with Miette
```rust
// Source: https://docs.rs/miette + https://docs.rs/strsim
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum ConfigError {
    #[error("unknown configuration key `{key}`")]
    #[diagnostic(
        code(blufio::config::unknown_key),
        help("did you mean `{suggestion}`? Valid keys: {valid_keys}")
    )]
    UnknownKey {
        key: String,
        suggestion: String,
        valid_keys: String,
        #[label("this key is not recognized")]
        span: SourceSpan,
        #[source_code]
        src: String,
    },

    #[error("invalid value for `{key}`")]
    #[diagnostic(code(blufio::config::invalid_value))]
    InvalidValue {
        key: String,
        expected: String,
        #[label("expected {expected}")]
        span: SourceSpan,
        #[source_code]
        src: String,
    },
}

/// Find the closest matching key using strsim
pub fn suggest_key(unknown: &str, valid_keys: &[&str]) -> Option<String> {
    valid_keys
        .iter()
        .filter_map(|k| {
            let dist = strsim::jaro_winkler(unknown, k);
            if dist > 0.8 { Some((*k, dist)) } else { None }
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(k, _)| k.to_string())
}
```

### Rust Toolchain File (rust-toolchain.toml)
```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `#[async_trait]` for all async traits | Native `async fn in trait` (Rust 1.75) | Dec 2023 | Native is faster (no Box allocation) BUT not dyn-compatible. Use native for non-dyn traits, async-trait for plugin traits that need trait objects. |
| Per-crate dependency versions | `workspace.dependencies` inheritance | Rust 1.64 (Sept 2022) | Define versions once in root Cargo.toml, inherit in members. Standard practice since 2023. |
| `workspace.package` not available | `workspace.package` metadata inheritance | Rust 1.64 (Sept 2022) | edition, license, version, authors shared across workspace members. |
| config-rs as only config library | figment as primary alternative | ~2021 (Rocket 0.5) | Figment's provenance tracking enables config error messages that point to exact source. Growing rapidly. |
| `actions-rs/*` GitHub Actions | `dtolnay/rust-toolchain` + `actions-rust-lang/*` | 2023-2024 | actions-rs is unmaintained. dtolnay/rust-toolchain + actions-rust-lang/audit are actively maintained replacements. |
| thiserror 1.x | thiserror 2.0 | Late 2024 | MSRV bump, but no breaking API changes for common usage. Use 2.0 for new projects. |
| `edition = "2021"` | `edition = "2024"` available | Rust 1.85 (Feb 2025) | Rust 2024 edition is stable. Enables new unsafe rules, gen blocks (nightly), and other ergonomic improvements. Use 2024 for new projects. |
| Manual Cargo.lock management | Cargo auto-generates for all workspace types | Ongoing | Always commit Cargo.lock for binary projects. Cargo workspace generates a single shared lockfile. |
| cross 0.1.x | cross 0.2.5 | 2024 | Added aarch64 runner support, improved musl target handling. Install from git for latest fixes. |
| cargo-deny 0.13.x | cargo-deny 0.16+ | 2024-2025 | New `[graph]` section replaces old target config. `deny.toml` format updated. |

**Deprecated/outdated:**
- `actions-rs/*` GitHub Actions: Unmaintained since 2023. Do NOT use. Replace with dtolnay/rust-toolchain + actions-rust-lang/*.
- `jemallocator` crate (without tikv- prefix): Deprecated. Use `tikv-jemallocator` 0.6.x.
- `config-rs` for new projects wanting provenance tracking: Still maintained but figment is the better choice for Elm-style error messages.
- Resolver v1 (default without `resolver = "2"`): Legacy behavior, always set resolver v2 explicitly in workspace.

## Open Questions

All three original open questions have been resolved by the deep dive research below. See the "Deep Dive" section for full details. Summary of resolutions:

1. **Figment-to-miette error bridge** -- RESOLVED: Figment errors do NOT contain line numbers or byte offsets. The `figment::Error` struct provides key paths (e.g., `agent.name`) and `Kind::UnknownField(field_name, &[expected_fields])` which gives us the unknown field and valid field list directly. To render TOML source context in miette, we must implement a custom bridge that: (a) reads the TOML source file, (b) searches for the key path to find byte offsets, (c) constructs miette `SourceSpan` from those offsets. This is a custom ~100-line module, not a library solution.

2. **Edition 2024 compatibility** -- RESOLVED: Safe to use `edition = "2024"`. The edition is a per-crate setting that only affects language syntax within that crate, not dependencies. Dependencies compiled with edition 2021 work fine in a 2024-edition workspace. All recommended crates (figment, tikv-jemallocator, miette, serde, etc.) are compatible because edition is not a transitive property.

3. **Tokio feature minimization** -- RESOLVED: blufio-core should NOT depend on tokio at all for Phase 1 stub traits. The `#[async_trait]` macro only needs `std::future::Future` and `std::pin::Pin`, not tokio. Tokio types (channels, I/O) belong in concrete implementations, not trait signatures. Use standard library types in trait signatures; add tokio dependency only when concrete implementations need it in later phases.

## Deep Dive: Figment Error Internals and Config Error UX

### Figment Error Type Anatomy (HIGH confidence -- docs.rs verified)

The `figment::Error` struct has four public fields:

```rust
pub struct Error {
    pub profile: Option<Profile>,     // Which config profile was active
    pub metadata: Option<Metadata>,   // Source provider metadata
    pub path: Vec<String>,            // Key path, e.g. ["agent", "name"]
    pub kind: Kind,                   // Error classification enum
}
```

The `Metadata` struct:
```rust
pub struct Metadata {
    pub name: Cow<'static, str>,                    // e.g., "TOML file"
    pub source: Option<Source>,                      // File path or custom source
    pub provide_location: Option<&'static Location<'static>>,  // Rust code location where provider was added
    // Private: interpolater function for path display
}
```

The critical `Kind` enum (for config error UX):
```rust
pub enum Kind {
    Message(String),
    InvalidType(Actual, String),           // Wrong type: "found string, expected u16"
    InvalidValue(Actual, String),          // Wrong value
    InvalidLength(usize, String),
    UnknownVariant(String, &'static [&'static str]),  // Unknown enum variant + valid variants
    UnknownField(String, &'static [&'static str]),    // CRITICAL: unknown field name + valid field names
    MissingField(Cow<'static, str>),
    DuplicateField(&'static str),
    ISizeOutOfRange(isize),
    USizeOutOfRange(usize),
    Unsupported(Actual),
    UnsupportedKey(Actual, Cow<'static, str>),
}
```

**Critical finding: `Kind::UnknownField` carries BOTH the unknown field name AND the list of valid field names.** This is the exact data needed for fuzzy matching typo suggestions -- we do NOT need to manually enumerate valid fields. When `deny_unknown_fields` triggers during deserialization, Figment captures the serde error and wraps it in `Kind::UnknownField(bad_key, &["valid_key_1", "valid_key_2", ...])`.

**What Figment does NOT provide:**
- Line numbers in the TOML source file
- Byte offsets / character positions
- Source file content (only file path via `Metadata::source`)

### The Figment-to-Miette Bridge Architecture (HIGH confidence)

Since Figment provides key paths but NOT source positions, we need a custom bridge module. The architecture:

```
Figment::extract() fails
    |
    v
figment::Error { path: ["agent", "naem"], kind: UnknownField("naem", &["name", "max_sessions"]) }
    |
    v
bridge module reads TOML source file from Metadata::source
    |
    v
bridge searches TOML text for key "naem" to find byte offset
    |
    v
bridge constructs miette::SourceSpan from byte offset
    |
    v
bridge runs strsim::jaro_winkler("naem", valid_fields) to find best suggestion
    |
    v
ConfigError::UnknownKey { key, suggestion, span, src } emitted as miette Diagnostic
```

**Implementation pattern:**
```rust
// Source: Custom pattern derived from figment::Error docs + miette docs
use figment::error::{Kind, Error as FigmentError};
use miette::{Diagnostic, SourceSpan, NamedSource, Report};
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum ConfigError {
    #[error("unknown configuration key `{key}`")]
    #[diagnostic(
        code(blufio::config::unknown_key),
        help("did you mean `{suggestion}`? Valid keys: {valid_keys}")
    )]
    UnknownKey {
        key: String,
        suggestion: String,
        valid_keys: String,
        #[label("this key is not recognized")]
        span: SourceSpan,
        #[source_code]
        src: NamedSource<String>,
    },

    #[error("invalid type for `{key}`: {detail}")]
    #[diagnostic(code(blufio::config::invalid_type))]
    InvalidType {
        key: String,
        detail: String,
        #[label("expected {expected}")]
        span: SourceSpan,
        #[source_code]
        src: NamedSource<String>,
    },

    #[error("missing required key `{key}`")]
    #[diagnostic(
        code(blufio::config::missing_key),
        help("add `{key} = <value>` to your blufio.toml")
    )]
    MissingKey {
        key: String,
    },

    #[error(transparent)]
    #[diagnostic(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Convert figment errors to miette-rendered config errors
pub fn figment_to_config_errors(
    err: FigmentError,
    toml_sources: &[(String, String)],  // Vec of (file_path, file_content)
) -> Vec<ConfigError> {
    err.into_iter().map(|e| {
        let key_path = e.path.join(".");

        match e.kind {
            Kind::UnknownField(ref field, expected) => {
                // Fuzzy match for suggestion
                let suggestion = suggest_key(field, expected)
                    .unwrap_or_else(|| "(none)".into());
                let valid_keys = expected.join(", ");

                // Find byte offset in TOML source
                if let Some((path, content)) = find_source(&e, toml_sources) {
                    if let Some(offset) = find_key_offset(content, &e.path, field) {
                        return ConfigError::UnknownKey {
                            key: field.clone(),
                            suggestion,
                            valid_keys,
                            span: SourceSpan::from(offset..offset + field.len()),
                            src: NamedSource::new(path, content.clone()),
                        };
                    }
                }

                // Fallback: no source location available
                ConfigError::UnknownKey {
                    key: field.clone(),
                    suggestion,
                    valid_keys,
                    span: SourceSpan::from(0..0),
                    src: NamedSource::new("(unknown)", String::new()),
                }
            },
            Kind::MissingField(ref field) => {
                ConfigError::MissingKey { key: field.to_string() }
            },
            _ => {
                ConfigError::Other(Box::new(
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
                ))
            }
        }
    }).collect()
}

/// Find the TOML source file that the error metadata points to
fn find_source<'a>(
    err: &figment::Error,
    sources: &'a [(String, String)],
) -> Option<(&'a str, &'a String)> {
    let meta = err.metadata.as_ref()?;
    let source = meta.source.as_ref()?;
    let path = source.custom()?;
    sources.iter()
        .find(|(p, _)| p.contains(path))
        .map(|(p, c)| (p.as_str(), c))
}

/// Find byte offset of a key in TOML content by key path
fn find_key_offset(content: &str, path: &[String], field: &str) -> Option<usize> {
    // For top-level keys in a section like [agent], search for the field name
    // after the section header
    let search_key = if path.len() > 1 {
        // Under a section: look for "[section]\n...key = "
        let section = &path[..path.len()-1].join(".");
        let section_header = format!("[{}]", section);
        let section_start = content.find(&section_header)?;
        let after_section = &content[section_start..];
        let key_in_section = after_section.find(field)?;
        section_start + key_in_section
    } else {
        // Top-level key
        content.find(field)?
    };
    Some(search_key)
}

/// Fuzzy match using Jaro-Winkler (best for short config key names)
pub fn suggest_key(unknown: &str, valid_keys: &[&str]) -> Option<String> {
    valid_keys
        .iter()
        .filter_map(|k| {
            let score = strsim::jaro_winkler(unknown, k);
            if score > 0.75 { Some((*k, score)) } else { None }
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(k, _)| k.to_string())
}
```

**Effort estimate:** ~150 lines of production code for the bridge module. This is the hardest UX piece in Phase 1 but the data is all available -- no missing information.

### Fuzzy Matching Algorithm Selection (HIGH confidence)

Use **Jaro-Winkler** (from `strsim` 0.11.x), not Levenshtein, for config key suggestions. Rationale:

| Algorithm | Best For | Score Range | Config Key Performance |
|-----------|----------|-------------|----------------------|
| Levenshtein | Fixed-cost edit distance | 0..inf (lower = better) | Poor for short strings with prefix matches |
| Jaro | Character transposition detection | 0.0..1.0 | Good but misses prefix weight |
| **Jaro-Winkler** | **Short strings with common prefixes** | **0.0..1.0** | **Best for config keys** (e.g., "naem" -> "name", "bot_tken" -> "bot_token") |
| Damerau-Levenshtein | Transpositions + insertions | 0..inf | Comparable to Jaro-Winkler but needs normalization |

Jaro-Winkler gives higher scores to strings that match from the beginning, which is exactly the typo pattern in config keys (users get the prefix right, mess up the suffix). Use a threshold of 0.75 (not 0.8 as in initial research) to catch more typos like `max_sesions` -> `max_sessions`.

### Figment Environment Variable Mapping -- The Split Ambiguity Problem (HIGH confidence)

**The problem is worse than initially documented.** The Figment maintainer (SergioBenitez) explicitly states in GitHub issue #12:

> "There does not exist an unambiguous way to split an environment variable name into nestings. For example, the name `A_B_C` could be `A[B][C]` or `A_B[C]` or `A[B_C]`."

This means `Env::prefixed("BLUFIO_").split("_")` is fundamentally broken for our config structure because keys like `bot_token` contain underscores.

**Solution: Use `Env::map()` with explicit key mapping instead of `split()`.**

```rust
// Source: figment docs + GitHub issue #12 discussion
use figment::providers::Env;

// WRONG: ambiguous split
// Env::prefixed("BLUFIO_").split("_")
// BLUFIO_TELEGRAM_BOT_TOKEN -> telegram.bot.token (WRONG!)

// RIGHT: explicit key mapping
fn env_provider() -> Env {
    Env::prefixed("BLUFIO_").map(|key| {
        // key is already lowercased and prefix-stripped by Env::prefixed
        // e.g., "telegram_bot_token" (from BLUFIO_TELEGRAM_BOT_TOKEN)
        key.as_str()
            .replacen("telegram_", "telegram.", 1)
            .replacen("anthropic_", "anthropic.", 1)
            .replacen("storage_", "storage.", 1)
            .replacen("security_", "security.", 1)
            .replacen("agent_", "agent.", 1)
            .replacen("cost_", "cost.", 1)
            .into()
    })
}
```

**Alternative: double-underscore convention.** Use `__` as the nesting separator:
```rust
// BLUFIO_TELEGRAM__BOT_TOKEN -> telegram.bot_token
Env::prefixed("BLUFIO_").split("__")
```

**Recommendation:** Use the explicit `map()` approach. It is unambiguous, self-documenting, and does not impose an ugly `__` convention on users. The `map()` approach knows exactly which config sections exist because they are defined in the code. The cost is maintaining the map function when sections change, but config sections change rarely.

**Updated workspace dependency note:** Remove the `split("_")` pattern from all code examples. Replace with `map()`.

### XDG Path Lookup Implementation (HIGH confidence)

The `dirs` crate (v6.x) is the correct choice. It provides `dirs::config_dir()` which returns:
- Linux: `$XDG_CONFIG_HOME` or `$HOME/.config`
- macOS: `$HOME/Library/Application Support`
- Windows: `{FOLDERID_RoamingAppData}` (not relevant, but handled)

For Blufio's XDG hierarchy, the Figment provider chain handles precedence natively:
```rust
use dirs;
use figment::providers::{Format, Toml};

// Layer 1: System config (lowest priority)
.merge(Toml::file("/etc/blufio/blufio.toml"))
// Layer 2: User config (XDG)
.merge(Toml::file(
    dirs::config_dir()
        .map(|d| d.join("blufio/blufio.toml"))
        .unwrap_or_default()
))
// Layer 3: Local config (highest file priority)
.merge(Toml::file("blufio.toml"))
// Layer 4: Env vars (highest overall priority)
.merge(env_provider())
```

Figment's `Toml::file()` silently skips missing files -- no error if `/etc/blufio/blufio.toml` does not exist. This is the correct behavior for optional config files.

## Deep Dive: musl + jemalloc Builds

### Cross-Compilation Strategy Decision (HIGH confidence)

**Use `cross-rs/cross` for CI musl builds. Use native compilation only for local development on Linux.**

| Approach | Pros | Cons | Best For |
|----------|------|------|----------|
| `cross-rs/cross` v0.2.5 | Docker-based, works on any host, handles all musl toolchain setup, pre-built images for x86_64/aarch64-musl | Requires Docker, ~2min overhead per build, some ARM runner issues | **CI release builds (recommended)** |
| Native `rustup target add x86_64-unknown-linux-musl` | No Docker needed, faster, simpler | Requires musl-tools installed, only works on Linux host, manual OpenSSL/ring setup | Local Linux development |
| `houseabsolute/actions-rust-cross` | GitHub Action wrapping cross | Less tested than direct cross usage | Alternative if cross CLI fails |
| `taiki-e/setup-cross-toolchain-action` | Newer, addresses cross pain points | Less community adoption | Watch list |

**macOS cannot natively compile musl targets.** `cargo build --target x86_64-unknown-linux-musl` on macOS requires Docker (via cross) or a Linux VM. This is expected and correct -- musl builds are CI-only (release tags), and CI runs on `ubuntu-latest`.

**Known cross-rs issues (2025):**
- ARM binary releases not published -- must install from git: `cargo install cross --git https://github.com/cross-rs/cross`
- Custom Docker images needed when cross-compiling FROM Linux ARM runners (not relevant for standard x86_64 CI)
- Docker-in-Docker can be problematic; GitHub-hosted runners handle this fine

### Jemalloc + musl Performance (HIGH confidence -- benchmarked)

From [raniz.blog 2025-02-06](https://raniz.blog/2025-02-06_rust-musl-malloc/):

| Allocator | Target | Multi-threaded Perf | vs glibc |
|-----------|--------|---------------------|----------|
| glibc default | linux-gnu | 56 ms (baseline) | 1.0x |
| musl default | linux-musl | 513 ms | **9.2x slower** |
| **jemalloc** | **linux-musl** | **67 ms** | **1.2x** |
| mimalloc | linux-musl | 57 ms | 1.02x |

**musl's default allocator is catastrophically slow under multi-threaded workloads** due to lock contention. Using jemalloc with musl is not optional -- it is required for acceptable performance. The CORE-06 requirement (jemalloc allocator) is even more critical than the PRD suggests.

**Configuration for conditional compilation:**
```toml
# In crates/blufio/Cargo.toml (binary crate only)
[target.'cfg(not(target_env = "msvc"))'.dependencies]
tikv-jemallocator = { workspace = true }
tikv-jemalloc-ctl = { workspace = true }
```

```rust
// In crates/blufio/src/main.rs
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;
```

### Binary Size Optimization (HIGH confidence)

Recommended release profile for the binary crate:

```toml
# In root Cargo.toml

# Standard release: optimized for speed + reasonable size
[profile.release]
opt-level = 3          # Maximum speed optimization
lto = "thin"           # Good LTO without extreme compile times
codegen-units = 1      # Better optimization, single codegen unit
strip = "debuginfo"    # Remove debug info, keep symbols for crash reports

# musl release: optimized for minimal binary size (CI release tags)
[profile.release-musl]
inherits = "release"
opt-level = "s"        # Optimize for size (often smaller than "z" in practice)
lto = true             # Fat LTO for maximum size reduction
panic = "abort"        # No unwinding tables, saves ~100-200KB
strip = "symbols"      # Remove all symbols for smallest binary
```

**Size impact breakdown (approximate, from multiple sources):**

| Setting | Impact |
|---------|--------|
| `opt-level = "s"` vs `3` | -15 to -25% binary size |
| `lto = true` (fat) vs `false` | -10 to -20% binary size |
| `codegen-units = 1` vs default | -5 to -10% binary size |
| `strip = "symbols"` | -20 to -40% binary size |
| `panic = "abort"` | -2 to -5% binary size |
| **Combined** | **-40 to -55% total** |

**Why `opt-level = "s"` instead of `"z"`:** The Rust internals discussion and real-world measurements show that `"s"` frequently produces smaller binaries than `"z"` because `"z"` disables vectorization and some inlining that can actually increase code size through less efficient code patterns. Always benchmark both, but default to `"s"`.

**Projected binary size:** PRD estimates 40-50MB for the full application with all features. Phase 1 skeleton (workspace + config + clap + jemalloc, no business logic) should be ~5-8MB with musl, or ~2-4MB with musl + all optimizations.

### TLS Strategy for musl Builds (HIGH confidence)

**Use rustls (not openssl) for all TLS needs.** This is a forward-looking decision for SEC-10 (TLS required for all remote connections) that must be locked in Phase 1 via cargo-deny.

| TLS Library | musl Compatibility | Binary Impact | Maintenance |
|-------------|-------------------|---------------|-------------|
| **rustls** | **Perfect -- pure Rust, no C deps** | **~1-2MB** | **Active, memory-safe** |
| openssl-sys | Broken -- requires libssl.so or vendored build, musl linking hell | ~3-5MB | C dependency, version conflicts |
| native-tls | Platform-dependent, not portable | Varies | Different behavior per platform |

**rustls uses `ring` for cryptography.** ring supports x86_64-unknown-linux-musl and aarch64-unknown-linux-musl, tested in their CI on every commit. As alternative, `aws-lc-rs` is a drop-in replacement for ring with FIPS support and pre-generated musl bindings.

**Phase 1 action:** The `cargo-deny.toml` already bans `openssl` and `openssl-sys`. This ensures no dependency pulls in OpenSSL accidentally. When reqwest/hyper/axum are added in later phases, always use the `rustls-tls` feature flag.

### musl 1.2.5 Update (MEDIUM confidence)

Starting with Rust 1.93 (stable 2026-01-22), all `*-linux-musl` targets ship with musl 1.2.5. Key improvements:
- Major DNS resolver improvements (critical for network-heavy agents)
- Better `time64` support on 32-bit platforms
- Improved threading performance (though still much slower than jemalloc)

Since we target latest stable Rust, we will get musl 1.2.5 automatically. No action needed.

## Deep Dive: Trait Architecture for Plugin System

### Trait Object vs Generics vs Associated Types (HIGH confidence)

**Decision: Use trait objects (`Box<dyn Trait>`) for all 7 adapter traits.** This is the only viable approach for a runtime plugin system.

| Pattern | Compile-time Known? | Runtime Swap? | Binary Bloat | Best For |
|---------|---------------------|---------------|--------------|----------|
| Generics (`T: Trait`) | Yes | No (monomorphized) | High (N copies) | Library APIs, zero-cost abstraction |
| Associated Types | Yes | No | Medium | Type-level configuration |
| **Trait Objects (`Box<dyn Trait>`)** | **No** | **Yes** | **Low (vtable)** | **Plugin systems, runtime configuration** |
| Enum Dispatch | Partially | If variants known | Low | Fixed set of implementations |

The plugin host needs to load adapter implementations at runtime based on configuration. `Box<dyn ChannelAdapter>` is the only pattern that allows "user configures `channel = "telegram"` in TOML, system loads Telegram adapter at runtime."

### Trait Hierarchy Design (HIGH confidence -- modeled after Tower/Axum)

**Study of real-world Rust trait hierarchies:**

**Tower's `Service` trait:**
```rust
pub trait Service<Request> {
    type Response;
    type Error;
    type Future: Future<Output = Result<Self::Response, Self::Error>>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>;
    fn call(&mut self, req: Request) -> Self::Future;
}
```
Tower uses associated types for Response/Error/Future. This works because Tower services are typically monomorphized at compile time. **This pattern does NOT work for our plugin system** because associated types make traits non-dyn-compatible when the types differ between implementations.

**Axum's `FromRequest` trait:**
```rust
pub trait FromRequest<S, M = ViaRequest>: Sized {
    type Rejection: IntoResponse;
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection>;
}
```
Axum uses `async fn` in traits (Rust 1.75+) because extractors are monomorphized per handler at compile time, not dispatched dynamically.

**The Blufio pattern must differ from both:**
- Unlike Tower: Cannot use associated error types (each adapter would have a different Error type, breaking dyn dispatch)
- Unlike Axum: Cannot use native async fn (must support dyn dispatch via trait objects)
- Like both: Use `Send + Sync + 'static` bounds

### Error Handling at Trait Boundaries (HIGH confidence)

**Use a concrete `BlufioError` enum (thiserror) as the error type for all adapter traits. Do NOT use `Box<dyn Error>` or `anyhow::Error`.**

Rationale:
- `Box<dyn Error + Send + Sync>` forces callers to downcast, losing all type information
- `anyhow::Error` couples all consumers to the anyhow crate, prevents major version upgrades
- A concrete enum lets match-based error handling, structured logging, and error categorization

```rust
// Source: Derived from thiserror docs + Rust error handling best practices (lpalmieri.com)
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BlufioError {
    // Infrastructure errors
    #[error("configuration error: {0}")]
    Config(String),

    #[error("storage error: {source}")]
    Storage {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("channel error: {message}")]
    Channel {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("provider error: {message}")]
    Provider {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("adapter not found: {adapter_type} adapter `{name}` is not registered")]
    AdapterNotFound {
        adapter_type: String,
        name: String,
    },

    #[error("adapter health check failed: {name}")]
    HealthCheckFailed {
        name: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("operation timed out after {duration:?}")]
    Timeout {
        duration: std::time::Duration,
    },

    #[error("internal error: {0}")]
    Internal(String),
}
```

**Why `Box<dyn Error + Send + Sync>` inside enum variants but not as the trait return type:** Individual adapters may have implementation-specific errors (reqwest::Error, rusqlite::Error, etc.). Wrapping these in `Box<dyn Error>` inside a categorized enum variant preserves both the category (Storage, Channel, Provider) and the original error chain. The caller matches on the variant for routing/logging, and the `.source()` chain provides full detail.

### Send + Sync + 'static Bounds (HIGH confidence)

All adapter traits MUST have `Send + Sync + 'static` supertraits:

```rust
#[async_trait]
pub trait PluginAdapter: Send + Sync + 'static { ... }
```

**Why each bound is required:**
- `Send`: Adapter objects will be moved between tokio tasks (e.g., health check task, message processing task)
- `Sync`: Multiple tasks may hold `&dyn PluginAdapter` references simultaneously (e.g., reading config, sending messages)
- `'static`: Adapter objects are stored in `Arc<dyn Trait>` in the plugin registry, which requires `'static`

**`#[async_trait]` default behavior:** The `#[async_trait]` macro adds `+ Send` to the returned future by default. Use `#[async_trait(?Send)]` only for single-threaded contexts, which Blufio does not use.

### Complete Trait Signature Pattern (HIGH confidence)

```rust
// Source: PRD Section 2.2 + async_trait docs + Tower/Axum patterns
use async_trait::async_trait;
use crate::error::BlufioError;

/// Adapter lifecycle states
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Unhealthy(String),
}

/// Adapter type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::Display, strum::EnumString)]
pub enum AdapterType {
    Channel,
    Provider,
    Storage,
    Embedding,
    Observability,
    Auth,
    SkillRuntime,
}

/// Base trait for ALL adapter plugins
#[async_trait]
pub trait PluginAdapter: Send + Sync + 'static {
    /// Human-readable adapter name (e.g., "telegram", "anthropic")
    fn name(&self) -> &str;

    /// Semantic version of this adapter
    fn version(&self) -> semver::Version;

    /// Which adapter slot this fills
    fn adapter_type(&self) -> AdapterType;

    /// Check adapter health (connectivity, resource availability)
    async fn health_check(&self) -> Result<HealthStatus, BlufioError>;

    /// Graceful shutdown -- release resources, close connections
    async fn shutdown(&self) -> Result<(), BlufioError>;
}

/// Example: Channel adapter trait (Phase 1 defines stubs only)
#[async_trait]
pub trait ChannelAdapter: PluginAdapter {
    /// Channel capabilities (text, images, voice, reactions, etc.)
    fn capabilities(&self) -> ChannelCapabilities;

    /// Connect to the messaging platform
    async fn connect(&mut self) -> Result<(), BlufioError>;

    /// Send a message to a conversation
    async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError>;

    /// Receive the next inbound message (long-poll or push)
    async fn receive(&self) -> Result<InboundMessage, BlufioError>;
}
```

**Note on `&mut self` vs `&self`:** `connect()` takes `&mut self` because it mutates internal state (establishing connection). All other methods take `&self` because the adapter is shared via `Arc<dyn ChannelAdapter>` after connection. This matches the Tower pattern where `poll_ready` takes `&mut self` but `call` logically borrows.

**Phase 1 stub implementations:** All 7 trait files are created with full signatures but `todo!()` method bodies. No concrete implementations until later phases.

### Avoiding Over-Abstraction: The "7 Traits, Not 70" Rule

From STATE.md: "Research recommends building Anthropic client directly in Phase 3, extracting provider trait -- not over-abstracting early."

Phase 1 trait stubs should be:
1. **Minimal method count** -- only methods that are clearly needed across all implementations of that type
2. **No generics on the traits** -- concrete types only (BlufioError, MessageId, etc.)
3. **No associated types** -- they break dyn compatibility
4. **Return types are concrete** -- `Result<MessageId, BlufioError>`, not `Result<Self::MessageId, Self::Error>`

## Deep Dive: Edition 2024 Compatibility

### How Rust Editions Work (HIGH confidence)

Rust editions are a **per-crate** setting, not a global workspace setting. Key facts:
- `edition = "2024"` in `workspace.package` applies to YOUR crates only
- Dependencies on crates.io are compiled with THEIR declared edition (typically 2021)
- **Editions are NOT transitive** -- a 2024-edition crate can depend on a 2015-edition crate with zero issues
- The Rust compiler supports ALL editions simultaneously in a single compilation

### Specific Crate Compatibility

| Crate | Compatible with Edition 2024? | Notes |
|-------|------------------------------|-------|
| figment 0.10.x | Yes | Edition only affects YOUR code syntax, not dependency compilation |
| serde 1.x | Yes | Same reason -- serde compiles with its own declared edition |
| tikv-jemallocator 0.6.x | Yes | Proc macros and C FFI unaffected by consumer edition |
| miette 7.x | Yes | Derive macros generate edition-appropriate code |
| thiserror 2.x | Yes | Same as above |
| async-trait 0.1.x | Yes | Proc macro generates code compatible with consumer edition |
| clap 4.x | Yes | No edition-specific syntax issues |

**Bottom line: Use `edition = "2024"` without hesitation.** The only risk is if your own code uses patterns that changed between 2021 and 2024 (e.g., `gen` is now a reserved keyword, `unsafe extern` blocks, changes to `!` never type). These are unlikely to cause issues in a new project.

### Edition 2024 Benefits for Blufio

- `unsafe_op_in_unsafe_fn` lint is deny-by-default (better safety for FFI with jemalloc)
- `gen` keyword reserved (future generators, not relevant now)
- Lifetime capture rules changes in `impl Trait` (clearer semantics)
- `tail_expr_drop_order` changes (more intuitive temporary drop behavior)

## Deep Dive: Tokio Feature Minimization

### Can blufio-core Avoid Tokio Entirely? (HIGH confidence)

**Yes -- and it SHOULD for Phase 1.** The `#[async_trait]` macro expands to:

```rust
// What you write:
#[async_trait]
pub trait PluginAdapter: Send + Sync + 'static {
    async fn health_check(&self) -> Result<HealthStatus, BlufioError>;
}

// What async_trait generates:
pub trait PluginAdapter: Send + Sync + 'static {
    fn health_check<'life0, 'async_trait>(
        &'life0 self,
    ) -> Pin<Box<dyn Future<Output = Result<HealthStatus, BlufioError>> + Send + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait;
}
```

The generated code only needs `std::pin::Pin`, `std::future::Future`, and `std::marker::Send` -- all from the standard library. **No tokio types appear in the expansion.**

Tokio types (`tokio::sync::mpsc`, `tokio::io::AsyncRead`) belong in:
- **Concrete struct fields** (the Telegram adapter's internal channel)
- **Implementation blocks** (the SQLite storage adapter's connection pool)
- **The binary crate's runtime** (`#[tokio::main]`)

They do NOT belong in trait signatures.

### Recommended Tokio Feature Configuration

```toml
# In root Cargo.toml [workspace.dependencies]
# Minimal: no features at workspace level
tokio = { version = "1" }

# In crates/blufio-core/Cargo.toml
# NO tokio dependency at all for Phase 1 stubs
[dependencies]
async-trait.workspace = true
thiserror.workspace = true
serde.workspace = true
semver.workspace = true
strum.workspace = true

# In crates/blufio-config/Cargo.toml
# NO tokio dependency needed
[dependencies]
figment.workspace = true
serde.workspace = true
miette.workspace = true
dirs.workspace = true
strsim.workspace = true

# In crates/blufio/Cargo.toml (binary crate)
# Full tokio for the runtime
[dependencies]
tokio = { workspace = true, features = ["full"] }
blufio-core = { path = "../blufio-core" }
blufio-config = { path = "../blufio-config" }
clap.workspace = true
tracing.workspace = true
```

**When tokio enters the picture (later phases):**
- Phase 2 (Persistence): `tokio = { workspace = true, features = ["sync"] }` in blufio-storage for `Mutex`, `RwLock`
- Phase 3 (Agent Loop): `tokio = { workspace = true, features = ["net", "io-util", "time"] }` in blufio-channel for async I/O
- Binary crate always: `features = ["full"]`

### Updated Workspace Dependencies

Based on all deep dive findings, the corrected workspace dependencies:

```toml
[workspace.dependencies]
# Serialization
serde = { version = "1", features = ["derive"] }

# Config
toml = "0.8"
figment = { version = "0.10", features = ["toml", "env"] }
dirs = "6"
strsim = "0.11"

# Error handling & diagnostics
thiserror = "2"
miette = { version = "7", features = ["fancy"] }

# CLI
clap = { version = "4.5", features = ["derive"] }

# Async traits (for plugin adapter dyn dispatch)
async-trait = "0.1"

# Type utilities
strum = { version = "0.26", features = ["derive"] }
semver = "1"

# Logging
tracing = "0.1"

# Runtime (NO features at workspace level -- each crate adds what it needs)
tokio = { version = "1" }

# Allocator (binary crate only, via target-specific deps)
tikv-jemallocator = "0.6"
tikv-jemalloc-ctl = "0.6"
```

**Key change from initial research:** `tokio` no longer has `features = ["full"]` at the workspace level. This prevents feature additivity bloat in library crates.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test framework (libtest) + cargo test |
| Config file | None needed -- Rust's test framework is built-in |
| Quick run command | `cargo test --workspace` |
| Full suite command | `cargo test --workspace -- --include-ignored` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CORE-05 | Binary compiles with musl linking | integration (CI only) | `cross build --release --target x86_64-unknown-linux-musl -p blufio` | N/A -- CI workflow |
| CORE-06 | Jemalloc is the global allocator | unit | `cargo test -p blufio -- jemalloc` | Wave 0 |
| CLI-06 | deny_unknown_fields rejects invalid keys | unit | `cargo test -p blufio-config -- deny_unknown` | Wave 0 |
| CLI-06 | Typo suggestions in error messages | unit | `cargo test -p blufio-config -- typo_suggest` | Wave 0 |
| INFRA-01 | SPDX headers on all .rs files | integration | `reuse lint` or custom script | Wave 0 |
| INFRA-02 | cargo-deny passes | integration (CI) | `cargo deny check --all-features` | Wave 0 |
| INFRA-03 | cargo-audit passes | integration (CI) | `cargo audit` | Wave 0 |
| INFRA-04 | Community docs exist | smoke | `test -f CONTRIBUTING.md && test -f CODE_OF_CONDUCT.md && test -f SECURITY.md && test -f GOVERNANCE.md` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --workspace`
- **Per wave merge:** `cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo deny check`
- **Phase gate:** Full CI pipeline green (fmt + clippy + test + deny + audit) before verify-work

### Wave 0 Gaps
- [ ] `crates/blufio-config/src/lib.rs` -- config parsing tests (deny_unknown_fields, typo suggestions, XDG lookup)
- [ ] `crates/blufio/src/main.rs` -- jemalloc allocator verification test
- [ ] `deny.toml` -- cargo-deny configuration file
- [ ] `.github/workflows/ci.yml` -- CI workflow for fmt/clippy/test/deny
- [ ] `.github/workflows/audit.yml` -- Audit workflow for cargo-audit

## Sources

### Primary (HIGH confidence)
- Context7: `/sergiobenitez/figment` -- config merging, Env::prefixed, Env::split, Env::map, Provider trait, Tagged value provenance, Error struct
- Context7: `/websites/serde_rs` -- deny_unknown_fields behavior, flatten incompatibility, field attributes
- Context7: `/websites/embarkstudios_github_io_cargo-deny` -- deny.toml configuration, license allow-list, clarify sections
- Context7: `/tower-rs/tower` -- Service trait pattern, Layer middleware, BoxService for dyn dispatch
- Context7: `/tokio-rs/axum` -- FromRequest trait design, Handler pattern, state management
- [figment::Error docs.rs](https://docs.rs/figment/latest/figment/struct.Error.html) -- Error struct fields: profile, metadata, path, kind
- [figment::error::Kind docs.rs](https://docs.rs/figment/latest/figment/error/enum.Kind.html) -- UnknownField variant with expected field names list
- [figment::Metadata docs.rs](https://docs.rs/figment/latest/figment/struct.Metadata.html) -- Source enum, name field, provide_location
- [figment::providers::Env docs.rs](https://docs.rs/figment/latest/figment/providers/struct.Env.html) -- prefixed, split, map, filter_map, only methods
- [Figment GitHub issue #12](https://github.com/SergioBenitez/Figment/issues/12) -- Split ambiguity with underscore-containing keys (maintainer response)
- [Cargo Workspaces - The Cargo Book](https://doc.rust-lang.org/cargo/reference/workspaces.html) -- workspace.dependencies, workspace.package inheritance, virtual manifest
- [tikv-jemallocator GitHub](https://github.com/tikv/jemallocator) -- v0.6.1, platform support, feature flags
- [dtolnay/rust-toolchain](https://github.com/dtolnay/rust-toolchain) -- GitHub Action for Rust toolchain
- [Swatinem/rust-cache](https://github.com/Swatinem/rust-cache) -- v2.7.7 cache action with configuration options
- [actions-rust-lang/audit](https://github.com/actions-rust-lang/audit) -- v1.2.7 audit action with issue creation
- [EmbarkStudios/cargo-deny-action](https://github.com/EmbarkStudios/cargo-deny-action) -- v2 deny action
- [Announcing async fn in traits](https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html) -- dyn incompatibility of native async fn in trait
- [tokio::sync docs.rs](https://docs.rs/tokio/latest/tokio/sync/index.html) -- sync primitives are runtime-agnostic, feature flags independent
- [Rust musl 1.2.5 update blog](https://blog.rust-lang.org/2025/12/05/Updating-musl-1.2.5/) -- DNS resolver improvements, Rust 1.93+

### Secondary (MEDIUM confidence)
- [Large Rust Workspaces (matklad)](https://matklad.github.io/2021/08/22/large-rust-workspaces.html) -- virtual manifest recommendation, crates/ directory pattern
- [Figment vs config-rs comparison](https://github.com/mehcode/config-rs/issues/371) -- provenance tracking advantage
- [Async trait dyn dispatch discussion](https://smallcultfollowing.com/babysteps/blog/2025/03/24/box-box-box/) -- current state of dyn async traits
- [REUSE tutorial](https://reuse.software/tutorial/) -- SPDX header format and reuse-tool usage
- [min-sized-rust](https://github.com/johnthagen/min-sized-rust) -- Release profile optimization settings
- [cross-rs/cross](https://github.com/cross-rs/cross) -- v0.2.5, Docker-based musl cross-compilation
- [raniz.blog: Performance of static Rust with MUSL](https://raniz.blog/2025-02-06_rust-musl-malloc/) -- jemalloc/mimalloc/musl allocator benchmarks
- [ring GitHub issue #713](https://github.com/briansmith/ring/issues/713) -- musl static linking support status
- [Definitive guide to Rust error handling](https://www.howtocodeit.com/articles/the-definitive-guide-to-rust-error-handling) -- thiserror vs anyhow at crate boundaries
- [Luca Palmieri: Error handling in Rust](https://lpalmieri.com/posts/error-handling-rust/) -- error type design for libraries
- [Cross compiling Rust in GitHub Actions](https://blog.urth.org/2023/03/05/cross-compiling-rust-projects-in-github-actions/) -- cross-rs practical CI patterns
- [Binary size optimization (Markaicode 2025)](https://markaicode.com/binary-size-optimization-techniques/) -- 43% reduction with combined optimizations

### Tertiary (LOW confidence)
- [houseabsolute/actions-rust-cross](https://github.com/houseabsolute/actions-rust-cross) -- GitHub Action wrapping cross-rs
- [taiki-e/setup-cross-toolchain-action](https://github.com/taiki-e/setup-cross-toolchain-action) -- newer alternative to cross, less community adoption
- [aws-lc-rs](https://github.com/aws/aws-lc-rs) -- ring alternative with FIPS support and musl pre-generated bindings

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- All libraries verified via Context7, crates.io, and official documentation. Versions confirmed current.
- Architecture: HIGH -- Trait pattern validated against Tower/Axum real-world patterns via Context7. dyn dispatch requirement confirmed. Error type design follows established Rust library conventions.
- Pitfalls: HIGH -- deny_unknown_fields+flatten confirmed via serde issue #1600. Jemalloc musl segfault confirmed via tikv/jemallocator#146. Env split ambiguity confirmed via Figment maintainer response in issue #12. Musl allocator performance confirmed via 2025 benchmarks.
- CI/CD: HIGH -- All GitHub Actions verified on their respective repositories with current version numbers.
- Config UX: HIGH (upgraded from MEDIUM) -- Figment Error internals fully documented from docs.rs. Kind::UnknownField confirmed to carry valid field names. Bridge architecture designed with concrete code example. No remaining unknowns.
- musl + jemalloc: HIGH -- Performance benchmarks from 2025 confirm jemalloc eliminates 9x musl slowdown. ring musl compatibility confirmed. rustls is pure Rust with no musl issues.
- Trait architecture: HIGH -- Validated against Tower Service trait and Axum FromRequest/Handler patterns. Error handling at trait boundaries follows established Rust library conventions.
- Tokio minimization: HIGH -- async_trait expansion confirmed to use only std types. blufio-core can be tokio-free.

**Research date:** 2026-02-28 (initial), 2026-02-28 (deep dive)
**Valid until:** 2026-03-28 (stable ecosystem, 30-day validity)
