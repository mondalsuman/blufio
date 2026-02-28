# Phase 1: Project Foundation & Workspace - Research

**Researched:** 2026-02-28
**Domain:** Rust project infrastructure -- Cargo workspace, config parsing, CI/CD, licensing, trait architecture
**Confidence:** HIGH

## Summary

Phase 1 establishes the entire build/test/quality infrastructure for Blufio from the first commit. The Rust ecosystem has well-established patterns for every requirement: Cargo workspace inheritance for multi-crate projects, figment for layered configuration with provenance-tracked error messages, tikv-jemallocator for jemalloc integration, and EmbarkStudios cargo-deny for license auditing. The critical architectural decision is defining 7 stub adapter traits in blufio-core using `#[async_trait]` (dtolnay) for dyn-dispatch compatibility, since native `async fn in trait` (stable since Rust 1.75) still cannot produce trait objects -- a hard requirement for the plugin architecture.

The biggest pitfall is the `#[serde(deny_unknown_fields)]` + `#[serde(flatten)]` incompatibility in serde. Since the config system needs `deny_unknown_fields` (requirement CLI-06) and may eventually need flattened structs, config structs must be designed flat from the start -- no `#[serde(flatten)]` anywhere. Figment handles the TOML + env var merging layer, serde handles the validation layer, and miette provides Elm-style diagnostic rendering for config errors.

**Primary recommendation:** Use figment 0.10 for config merging (TOML + env + defaults), serde with `deny_unknown_fields` on all config structs (no flatten), miette for Elm-style error display, `#[async_trait]` for all adapter trait definitions, and virtual Cargo workspace manifest with `workspace.dependencies` inheritance.

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
tokio = { version = "1", features = ["full"] }
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
        .merge(Env::prefixed("BLUFIO_").split("_"))
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

### Pitfall 5: Figment Env Split Depth
**What goes wrong:** `Env::prefixed("BLUFIO_").split("_")` with deeply nested config keys causes incorrect key mapping. `BLUFIO_AGENT_MAX_SESSIONS` maps to `agent.max.sessions` instead of `agent.max_sessions`.
**Why it happens:** The split delimiter `_` is ambiguous when config keys themselves contain underscores. Figment splits on every occurrence.
**How to avoid:** Use double underscore `__` as the split delimiter: `Env::prefixed("BLUFIO_").split("__")`. This means env vars use `BLUFIO_AGENT__MAX_SESSIONS` for `agent.max_sessions`. Alternatively, keep config flat (one level of nesting only, as decided) and map env vars explicitly.
**Warning signs:** Config values appearing under wrong keys when set via environment variables.

### Pitfall 6: Missing `resolver = "2"` in Workspace
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

1. **Figment + deny_unknown_fields interaction specifics**
   - What we know: Figment uses serde for extraction, and `deny_unknown_fields` works on the serde layer. Figment's own error reporting adds provenance on top.
   - What's unclear: Whether Figment's error type preserves enough information (source spans, line numbers) for miette to render with TOML source context. May need a custom bridge between Figment errors and miette diagnostics.
   - Recommendation: Implement a `FigmentToMiette` error conversion in blufio-config that reads the TOML source file and maps Figment's key paths to byte offsets for miette SourceSpan. Test this early in Phase 1.

2. **Edition 2024 compatibility with workspace tooling**
   - What we know: Rust 2024 edition is stable since Rust 1.85 (Feb 2025). Cargo-deny, clippy, and other tools should support it.
   - What's unclear: Whether all dependencies compile cleanly under edition 2024, and whether any CI actions have issues.
   - Recommendation: Start with `edition = "2024"` in workspace. If any issues emerge, fall back to `edition = "2021"`. The edition is per-crate so it can be changed easily.

3. **Tokio feature minimization for library crates**
   - What we know: blufio-core defines async traits that depend on tokio types (e.g., `tokio::sync::mpsc`). But not all traits need all tokio features.
   - What's unclear: The minimal tokio feature set needed for trait signatures in blufio-core vs the full set needed in the binary crate.
   - Recommendation: Start with `tokio = { version = "1" }` (no features) in workspace.dependencies. Add features per-crate as needed. The binary crate adds `features = ["full"]`.

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
- Context7: `/sergiobenitez/figment` -- config merging, Env::prefixed, Env::split, Provider trait
- Context7: `/websites/serde_rs` -- deny_unknown_fields behavior, flatten incompatibility, field attributes
- Context7: `/websites/embarkstudios_github_io_cargo-deny` -- deny.toml configuration, license allow-list, clarify sections
- [Cargo Workspaces - The Cargo Book](https://doc.rust-lang.org/cargo/reference/workspaces.html) -- workspace.dependencies, workspace.package inheritance, virtual manifest
- [tikv-jemallocator GitHub](https://github.com/tikv/jemallocator) -- v0.6.1, platform support, feature flags
- [dtolnay/rust-toolchain](https://github.com/dtolnay/rust-toolchain) -- GitHub Action for Rust toolchain
- [Swatinem/rust-cache](https://github.com/Swatinem/rust-cache) -- v2.7.7 cache action with configuration options
- [actions-rust-lang/audit](https://github.com/actions-rust-lang/audit) -- v1.2.7 audit action with issue creation
- [EmbarkStudios/cargo-deny-action](https://github.com/EmbarkStudios/cargo-deny-action) -- v2 deny action
- [Announcing async fn in traits](https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html) -- dyn incompatibility of native async fn in trait

### Secondary (MEDIUM confidence)
- [Large Rust Workspaces (matklad)](https://matklad.github.io/2021/08/22/large-rust-workspaces.html) -- virtual manifest recommendation, crates/ directory pattern, version = "0.0.0"
- [Figment vs config-rs comparison](https://github.com/mehcode/config-rs/issues/371) -- provenance tracking advantage
- [Async trait dyn dispatch discussion](https://smallcultfollowing.com/babysteps/blog/2025/03/24/box-box-box/) -- current state of dyn async traits
- [REUSE tutorial](https://reuse.software/tutorial/) -- SPDX header format and reuse-tool usage
- [min-sized-rust](https://github.com/johnthagen/min-sized-rust) -- Release profile optimization settings
- [cross-rs/cross](https://github.com/cross-rs/cross) -- v0.2.5, Docker-based musl cross-compilation

### Tertiary (LOW confidence)
- [houseabsolute/actions-rust-cross](https://github.com/houseabsolute/actions-rust-cross) -- GitHub Action wrapping cross-rs, fewer direct verifications
- Rust 2024 edition compatibility with all workspace tooling -- not extensively tested in production reports yet

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- All libraries verified via Context7, crates.io, and official documentation. Versions confirmed current.
- Architecture: HIGH -- Workspace patterns from Cargo Book (official docs). Trait pattern from PRD + Rust reference on async trait limitations.
- Pitfalls: HIGH -- deny_unknown_fields+flatten confirmed via serde issue #1600 and official docs. Jemalloc musl segfault confirmed via tikv/jemallocator#146. Feature additivity from Cargo Book.
- CI/CD: HIGH -- All GitHub Actions verified on their respective repositories with current version numbers.
- Config merging: MEDIUM -- Figment-to-miette bridge for TOML source spans is theoretically sound but not verified with a working example. Flagged in Open Questions.

**Research date:** 2026-02-28
**Valid until:** 2026-03-28 (stable ecosystem, 30-day validity)
