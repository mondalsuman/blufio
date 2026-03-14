// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio bench` command implementation.
//!
//! Runs built-in performance benchmarks (startup, context assembly, WASM,
//! SQLite) and reports median timing with peak RSS, system info header,
//! and table output. Results are stored in the bench_results SQLite table
//! for regression tracking.

use std::fmt;
use std::str::FromStr;
use std::time::{Duration, Instant};

use blufio_core::BlufioError;
use serde::{Deserialize, Serialize};

/// A single benchmark result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// Name of the benchmark.
    pub name: String,
    /// Median duration across iterations.
    pub median: Duration,
    /// Minimum duration across iterations.
    pub min: Duration,
    /// Maximum duration across iterations.
    pub max: Duration,
    /// Peak resident set size in bytes (if available).
    pub peak_rss: Option<u64>,
    /// Number of measured iterations.
    pub iterations: u32,
}

/// The kinds of built-in benchmarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BenchmarkKind {
    Startup,
    ContextAssembly,
    Wasm,
    Sqlite,
    BinarySize,
    MemoryProfile,
}

impl fmt::Display for BenchmarkKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BenchmarkKind::Startup => write!(f, "startup"),
            BenchmarkKind::ContextAssembly => write!(f, "context"),
            BenchmarkKind::Wasm => write!(f, "wasm"),
            BenchmarkKind::Sqlite => write!(f, "sqlite"),
            BenchmarkKind::BinarySize => write!(f, "binary_size"),
            BenchmarkKind::MemoryProfile => write!(f, "memory_profile"),
        }
    }
}

impl FromStr for BenchmarkKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "startup" => Ok(BenchmarkKind::Startup),
            "context" | "context_assembly" | "contextassembly" => {
                Ok(BenchmarkKind::ContextAssembly)
            }
            "wasm" => Ok(BenchmarkKind::Wasm),
            "sqlite" => Ok(BenchmarkKind::Sqlite),
            "binary_size" | "binarysize" | "binary" => Ok(BenchmarkKind::BinarySize),
            "memory_profile" | "memoryprofile" | "memory" => Ok(BenchmarkKind::MemoryProfile),
            _ => Err(format!("unknown benchmark: {s}")),
        }
    }
}

/// Collect system information for the benchmark header.
fn collect_system_info() -> serde_json::Value {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let cpu_count = sys.cpus().len();
    let total_ram_mb = sys.total_memory() / (1024 * 1024);
    let os_name = System::name().unwrap_or_else(|| "unknown".to_string());
    let os_version = System::os_version().unwrap_or_else(|| "unknown".to_string());

    serde_json::json!({
        "cpu": cpu_name,
        "cores": cpu_count,
        "ram_mb": total_ram_mb,
        "os": format!("{os_name} {os_version}"),
        "blufio_version": env!("CARGO_PKG_VERSION"),
    })
}

/// Get peak RSS (resident set size) using platform-specific APIs.
fn get_peak_rss() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        // macOS: getrusage returns ru_maxrss in bytes
        use std::mem::MaybeUninit;
        let mut usage = MaybeUninit::<libc::rusage>::uninit();
        let ret = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
        if ret == 0 {
            let usage = unsafe { usage.assume_init() };
            // macOS reports in bytes
            Some(usage.ru_maxrss as u64)
        } else {
            None
        }
    }
    #[cfg(target_os = "linux")]
    {
        // Linux: read /proc/self/status VmHWM line
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmHWM:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2
                        && let Ok(kb) = parts[1].parse::<u64>()
                    {
                        return Some(kb * 1024);
                    }
                }
            }
        }
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Run a benchmark of the given kind with the specified number of iterations.
fn run_benchmark(kind: BenchmarkKind, iterations: u32) -> Result<BenchmarkResult, BlufioError> {
    // 1 warm-up run (discarded)
    run_single_benchmark(kind)?;

    // N measured runs
    let mut timings = Vec::with_capacity(iterations as usize);
    for _ in 0..iterations {
        let elapsed = run_single_benchmark(kind)?;
        timings.push(elapsed);
    }

    // Measure peak RSS after benchmark completes
    let peak_rss = get_peak_rss();

    // Sort for median/min/max
    timings.sort();

    let median = timings[timings.len() / 2];
    let min = timings[0];
    let max = timings[timings.len() - 1];

    Ok(BenchmarkResult {
        name: kind.to_string(),
        median,
        min,
        max,
        peak_rss,
        iterations,
    })
}

/// Run a single iteration of a benchmark, returning its duration.
///
/// Note: `BinarySize` and `MemoryProfile` are not timing-based benchmarks and
/// are handled directly in `run_bench()` — they should never reach this function.
fn run_single_benchmark(kind: BenchmarkKind) -> Result<Duration, BlufioError> {
    match kind {
        BenchmarkKind::Startup => bench_startup(),
        BenchmarkKind::ContextAssembly => bench_context_assembly(),
        BenchmarkKind::Wasm => bench_wasm(),
        BenchmarkKind::Sqlite => bench_sqlite(),
        BenchmarkKind::BinarySize | BenchmarkKind::MemoryProfile => {
            unreachable!("BinarySize and MemoryProfile are handled directly in run_bench()")
        }
    }
}

/// Benchmark: measure time to load config and initialize components.
fn bench_startup() -> Result<Duration, BlufioError> {
    let start = Instant::now();
    let _ = blufio_config::load_and_validate();
    Ok(start.elapsed())
}

/// Benchmark: measure context window assembly time with synthetic messages.
fn bench_context_assembly() -> Result<Duration, BlufioError> {
    let start = Instant::now();

    // Create synthetic deterministic messages to measure assembly overhead
    let mut messages = Vec::with_capacity(100);
    for i in 0..100 {
        messages.push(serde_json::json!({
            "role": if i % 2 == 0 { "user" } else { "assistant" },
            "content": format!("Synthetic message {} for benchmarking context assembly. This simulates a typical conversation message with enough content to be realistic.", i),
        }));
    }

    // Serialize and measure the assembly of a large context
    let _assembled = serde_json::to_string(&messages)
        .map_err(|e| BlufioError::Internal(format!("context assembly benchmark failed: {e}")))?;

    Ok(start.elapsed())
}

/// Benchmark: measure WASM module load time.
fn bench_wasm() -> Result<Duration, BlufioError> {
    let start = Instant::now();

    // Create a minimal valid WASM module (magic number + version + empty)
    let minimal_wasm: &[u8] = &[
        0x00, 0x61, 0x73, 0x6D, // magic: \0asm
        0x01, 0x00, 0x00, 0x00, // version: 1
    ];

    // Measure the time to validate WASM bytes
    let _ = wasmtime::Module::validate(&wasmtime::Engine::default(), minimal_wasm);

    Ok(start.elapsed())
}

/// Benchmark: measure batch insert + query operations on a temporary SQLite database.
fn bench_sqlite() -> Result<Duration, BlufioError> {
    let start = Instant::now();

    let dir = tempfile::tempdir().map_err(|e| {
        BlufioError::Internal(format!(
            "failed to create temp dir for sqlite benchmark: {e}"
        ))
    })?;
    let db_path = dir.path().join("bench.db");

    let conn =
        rusqlite::Connection::open(&db_path).map_err(BlufioError::storage_connection_failed)?;

    conn.execute_batch("CREATE TABLE bench_data (id INTEGER PRIMARY KEY, value TEXT NOT NULL);")
        .map_err(BlufioError::storage_connection_failed)?;

    // Batch insert 1000 rows
    conn.execute_batch("BEGIN TRANSACTION;")
        .map_err(BlufioError::storage_connection_failed)?;

    for i in 0..1000 {
        conn.execute(
            "INSERT INTO bench_data (id, value) VALUES (?1, ?2)",
            rusqlite::params![i, format!("benchmark-value-{i}")],
        )
        .map_err(BlufioError::storage_connection_failed)?;
    }

    conn.execute_batch("COMMIT;")
        .map_err(BlufioError::storage_connection_failed)?;

    // Query all rows
    let mut stmt = conn
        .prepare("SELECT id, value FROM bench_data ORDER BY id")
        .map_err(BlufioError::storage_connection_failed)?;

    let count: usize = stmt
        .query_map([], |row| {
            let _id: i64 = row.get(0)?;
            let _val: String = row.get(1)?;
            Ok(())
        })
        .map_err(BlufioError::storage_connection_failed)?
        .filter_map(|r| r.ok())
        .count();

    drop(stmt);
    drop(conn);

    if count != 1000 {
        return Err(BlufioError::Internal(format!(
            "sqlite benchmark: expected 1000 rows, got {count}"
        )));
    }

    Ok(start.elapsed())
}

/// Sample current jemalloc resident memory by advancing the epoch and reading stats.
///
/// Returns the resident (RSS-like) value in bytes. Must call `epoch::advance()` first
/// to get a fresh snapshot.
fn sample_rss() -> u64 {
    use tikv_jemalloc_ctl::{epoch, stats};

    let _ = epoch::advance();
    stats::resident::read().unwrap_or(0) as u64
}

/// Check RSS samples for a potential memory leak.
///
/// A leak is flagged when RSS grows monotonically (every sample >= previous)
/// AND total growth exceeds 10% of the initial measurement.
fn check_leak(samples: &[u64]) {
    if samples.len() < 2 {
        return;
    }

    let is_monotonic = samples.windows(2).all(|w| w[1] >= w[0]);
    let first = samples[0];
    let last = *samples.last().unwrap();

    if first == 0 {
        return;
    }

    let growth_pct = ((last as f64 - first as f64) / first as f64) * 100.0;

    if is_monotonic && growth_pct > 10.0 {
        eprintln!(
            "  WARNING: Potential memory leak detected -- RSS grew monotonically from {} to {} ({:.1}%)",
            format_bytes(first),
            format_bytes(last),
            growth_pct,
        );
    }
}

/// Print a summary of RSS samples: min, max, mean, and growth trend.
fn print_rss_summary(samples: &[u64]) {
    if samples.is_empty() {
        return;
    }

    let min = *samples.iter().min().unwrap();
    let max = *samples.iter().max().unwrap();
    let mean = samples.iter().sum::<u64>() / samples.len() as u64;
    let first = samples[0];
    let last = *samples.last().unwrap();

    let trend = if last > first {
        format!("+{}", format_bytes(last - first))
    } else if last < first {
        format!("-{}", format_bytes(first - last))
    } else {
        "stable".to_string()
    };

    eprintln!("  RSS samples:  min={}, max={}, mean={}, trend={trend}",
        format_bytes(min), format_bytes(max), format_bytes(mean));
}

/// Benchmark: measure memory profile using jemalloc stats.
///
/// Reports idle memory stats (allocated, active, resident, mapped) via jemalloc,
/// includes RSS sampling helpers for leak detection under load, and prints
/// comparison against targets and OpenClaw baseline.
fn bench_memory_profile(json: bool) -> Result<BenchmarkResult, BlufioError> {
    use tikv_jemalloc_ctl::{epoch, stats};

    // === Idle Memory Profile ===
    // Advance the jemalloc epoch to get fresh stats
    epoch::advance().map_err(|e| {
        BlufioError::Internal(format!("jemalloc epoch::advance() failed: {e}"))
    })?;

    let allocated = stats::allocated::read().map_err(|e| {
        BlufioError::Internal(format!("jemalloc stats::allocated::read() failed: {e}"))
    })?;
    let active = stats::active::read().map_err(|e| {
        BlufioError::Internal(format!("jemalloc stats::active::read() failed: {e}"))
    })?;
    let resident = stats::resident::read().map_err(|e| {
        BlufioError::Internal(format!("jemalloc stats::resident::read() failed: {e}"))
    })?;
    let mapped = stats::mapped::read().map_err(|e| {
        BlufioError::Internal(format!("jemalloc stats::mapped::read() failed: {e}"))
    })?;

    // OS-level peak RSS via getrusage / /proc/self/status
    let peak_rss = get_peak_rss();

    if !json {
        eprintln!();
        eprintln!("  === Idle Memory Profile ===");
        eprintln!("  Allocated: {}", format_bytes(allocated as u64));
        eprintln!("  Active:    {}", format_bytes(active as u64));
        eprintln!("  Resident:  {}", format_bytes(resident as u64));
        eprintln!("  Mapped:    {}", format_bytes(mapped as u64));
        if let Some(rss) = peak_rss {
            eprintln!("  Peak RSS:  {} (OS-level)", format_bytes(rss));
        }

        // Target comparison: 50-80MB idle
        let resident_mb = resident as f64 / (1024.0 * 1024.0);
        let status = if resident_mb < 50.0 {
            "BELOW"
        } else if resident_mb <= 80.0 {
            "WITHIN"
        } else {
            "ABOVE"
        };
        eprintln!(
            "  Target: 50-80MB idle | Measured: {:.1}MB | Status: {status}",
            resident_mb
        );

        // OpenClaw comparison
        eprintln!("  OpenClaw documented range: 300-800MB");

        // vec0 comparison hint
        eprintln!();
        eprintln!(
            "  Tip: For vec0 vs in-memory comparison, run with --vec0-enabled=true \
             and --vec0-enabled=false back-to-back"
        );

        // Under-load RSS sampling framework (idle-only for now; the framework is ready
        // for Plan 04's CI integration with full workload).
        eprintln!();
        eprintln!("  === Under-load RSS Sampling (framework ready) ===");
        eprintln!(
            "  The RSS sampling framework (sample_rss, check_leak, print_rss_summary) \
             is available."
        );
        eprintln!(
            "  Full under-load measurement (1000 saves + 100 retrievals) will be \
             exercised when invoked with the full application context."
        );

        // Demonstrate the framework with a quick idle sample sequence
        let idle_samples: Vec<u64> = (0..5).map(|_| sample_rss()).collect();
        print_rss_summary(&idle_samples);
        check_leak(&idle_samples);

        eprintln!();
    }

    Ok(BenchmarkResult {
        name: "memory_profile".to_string(),
        median: Duration::ZERO,
        min: Duration::ZERO,
        max: Duration::ZERO,
        peak_rss: Some(resident as u64),
        iterations: 1,
    })
}

/// Benchmark: measure binary file size and optionally run cargo-bloat for per-crate breakdown.
///
/// Returns a `BenchmarkResult` with the binary size stored in `peak_rss` (repurposed).
fn bench_binary_size(json: bool) -> Result<BenchmarkResult, BlufioError> {
    let exe_path = std::env::current_exe()
        .map_err(|e| BlufioError::Internal(format!("cannot locate own binary: {e}")))?;
    let metadata = std::fs::metadata(&exe_path)
        .map_err(|e| BlufioError::Internal(format!("cannot stat binary: {e}")))?;
    let size_bytes = metadata.len();

    if !json {
        eprintln!();
        eprintln!("  === Binary Size Report ===");
        eprintln!("  Path:   {}", exe_path.display());
        eprintln!("  Size:   {} ({size_bytes} bytes)", format_bytes(size_bytes));

        // Target comparison: <50MB
        let target_mb: u64 = 50;
        let status = if size_bytes < target_mb * 1024 * 1024 {
            "OK"
        } else {
            "EXCEEDED"
        };
        eprintln!(
            "  Target: <{target_mb}MB | Status: {status}"
        );

        // Debug vs release detection
        if cfg!(debug_assertions) {
            eprintln!(
                "  Note:   Debug build detected -- release size will differ"
            );
        }

        // Attempt cargo-bloat per-crate breakdown (report-only)
        eprintln!();
        eprint!("  Per-crate breakdown (cargo-bloat)...");
        match std::process::Command::new("cargo")
            .args(["bloat", "--release", "--crates", "-n", "20"])
            .output()
        {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                eprintln!();
                for line in stdout.lines() {
                    eprintln!("    {line}");
                }
            }
            _ => {
                eprintln!(
                    " not available -- run `cargo install cargo-bloat` for per-crate breakdown"
                );
            }
        }
        eprintln!();
    }

    Ok(BenchmarkResult {
        name: "binary_size".to_string(),
        median: Duration::ZERO,
        min: Duration::ZERO,
        max: Duration::ZERO,
        peak_rss: Some(size_bytes),
        iterations: 1,
    })
}

/// Format a duration for display.
fn format_duration(d: Duration) -> String {
    let nanos = d.as_nanos();
    if nanos < 1_000 {
        format!("{nanos}ns")
    } else if nanos < 1_000_000 {
        format!("{:.1}us", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2}ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2}s", nanos as f64 / 1_000_000_000.0)
    }
}

/// Format bytes for display.
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Save benchmark results to the SQLite database.
#[cfg(feature = "sqlite")]
fn save_results(
    db_path: &str,
    results: &[BenchmarkResult],
    system_info: &serde_json::Value,
    is_baseline: bool,
) -> Result<(), BlufioError> {
    let path = std::path::Path::new(db_path);
    if !path.exists() {
        // Database doesn't exist yet; skip storage (no error)
        eprintln!("  (skipping result storage: database not found)");
        return Ok(());
    }

    let conn = blufio_storage::open_connection_sync(db_path, rusqlite::OpenFlags::default())?;

    let system_info_str = serde_json::to_string(system_info).unwrap_or_default();

    if is_baseline {
        // Clear previous baselines
        conn.execute("UPDATE bench_results SET is_baseline = 0", [])
            .map_err(BlufioError::storage_connection_failed)?;
    }

    for result in results {
        conn.execute(
            "INSERT INTO bench_results (benchmark, median_ns, min_ns, max_ns, peak_rss_bytes, iterations, system_info, is_baseline) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                result.name,
                result.median.as_nanos() as i64,
                result.min.as_nanos() as i64,
                result.max.as_nanos() as i64,
                result.peak_rss.map(|v| v as i64),
                result.iterations as i64,
                system_info_str,
                if is_baseline { 1 } else { 0 },
            ],
        )
        .map_err(BlufioError::storage_connection_failed)?;
    }

    Ok(())
}

/// Save benchmark results (no-op without sqlite feature).
#[cfg(not(feature = "sqlite"))]
fn save_results(
    _db_path: &str,
    _results: &[BenchmarkResult],
    _system_info: &serde_json::Value,
    _is_baseline: bool,
) -> Result<(), BlufioError> {
    eprintln!("  (result storage requires sqlite feature)");
    Ok(())
}

/// Load previous benchmark results for comparison.
#[cfg(feature = "sqlite")]
fn load_previous_results(db_path: &str) -> Result<Vec<(String, i64)>, BlufioError> {
    let path = std::path::Path::new(db_path);
    if !path.exists() {
        return Ok(vec![]);
    }

    let conn = blufio_storage::open_connection_sync(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    // Check if bench_results table exists
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='bench_results'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !table_exists {
        return Ok(vec![]);
    }

    let mut stmt = conn.prepare(
        "SELECT benchmark, median_ns FROM bench_results WHERE id IN (SELECT MAX(id) FROM bench_results WHERE is_baseline = 0 GROUP BY benchmark) ORDER BY benchmark"
    ).map_err(BlufioError::storage_connection_failed)?;

    let results: Vec<(String, i64)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(BlufioError::storage_connection_failed)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(results)
}

/// Load previous benchmark results (no-op without sqlite feature).
#[cfg(not(feature = "sqlite"))]
fn load_previous_results(_db_path: &str) -> Result<Vec<(String, i64)>, BlufioError> {
    Ok(vec![])
}

/// Load baseline results for CI comparison.
#[cfg(feature = "sqlite")]
fn load_baseline_results(db_path: &str) -> Result<Vec<(String, i64)>, BlufioError> {
    let path = std::path::Path::new(db_path);
    if !path.exists() {
        return Ok(vec![]);
    }

    let conn = blufio_storage::open_connection_sync(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    // Check if bench_results table exists
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='bench_results'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !table_exists {
        return Ok(vec![]);
    }

    let mut stmt = conn.prepare(
        "SELECT benchmark, median_ns FROM bench_results WHERE is_baseline = 1 ORDER BY benchmark"
    ).map_err(BlufioError::storage_connection_failed)?;

    let results: Vec<(String, i64)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(BlufioError::storage_connection_failed)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(results)
}

/// Load baseline results (no-op without sqlite feature).
#[cfg(not(feature = "sqlite"))]
fn load_baseline_results(_db_path: &str) -> Result<Vec<(String, i64)>, BlufioError> {
    Ok(vec![])
}

/// Main entry point for `blufio bench`.
pub async fn run_bench(
    only: Option<Vec<String>>,
    json: bool,
    compare: bool,
    baseline: bool,
    iterations: Option<u32>,
    ci: bool,
    threshold: Option<f64>,
) -> Result<(), BlufioError> {
    let config = blufio_config::load_and_validate()
        .map_err(|errors| BlufioError::Config(format!("{} config error(s)", errors.len())))?;

    let iterations = iterations.unwrap_or(3);
    let threshold_pct = threshold.unwrap_or(20.0);

    // Determine which benchmarks to run
    let all_kinds = vec![
        BenchmarkKind::Startup,
        BenchmarkKind::ContextAssembly,
        BenchmarkKind::Wasm,
        BenchmarkKind::Sqlite,
        BenchmarkKind::BinarySize,
        BenchmarkKind::MemoryProfile,
    ];

    let selected: Vec<BenchmarkKind> = if let Some(ref only_list) = only {
        let mut selected = Vec::new();
        for name in only_list {
            for part in name.split(',') {
                let kind = BenchmarkKind::from_str(part.trim()).map_err(BlufioError::Internal)?;
                selected.push(kind);
            }
        }
        selected
    } else {
        all_kinds
    };

    // Collect system info
    let system_info = collect_system_info();

    if !json {
        eprintln!();
        eprintln!("  blufio bench");
        eprintln!("  {}", "-".repeat(50));
        eprintln!(
            "  CPU:     {}",
            system_info["cpu"].as_str().unwrap_or("unknown")
        );
        eprintln!("  Cores:   {}", system_info["cores"].as_u64().unwrap_or(0));
        eprintln!(
            "  RAM:     {} MB",
            system_info["ram_mb"].as_u64().unwrap_or(0)
        );
        eprintln!(
            "  OS:      {}",
            system_info["os"].as_str().unwrap_or("unknown")
        );
        eprintln!(
            "  Blufio:  v{}",
            system_info["blufio_version"].as_str().unwrap_or("?")
        );
        eprintln!("  Iters:   {iterations}");
        eprintln!();
    }

    // Run benchmarks
    let mut results = Vec::new();
    for kind in &selected {
        // BinarySize and MemoryProfile are not timing-based benchmarks;
        // they have dedicated implementations that bypass the iteration loop.
        match kind {
            BenchmarkKind::BinarySize => {
                if !json {
                    eprint!("  Running {kind}...");
                }
                match bench_binary_size(json) {
                    Ok(result) => {
                        if !json {
                            eprintln!(
                                " {}",
                                result.peak_rss.map(format_bytes).unwrap_or_default()
                            );
                        }
                        results.push(result);
                    }
                    Err(e) => {
                        if !json {
                            eprintln!(" FAILED: {e}");
                        }
                    }
                }
            }
            BenchmarkKind::MemoryProfile => {
                if !json {
                    eprint!("  Running {kind}...");
                }
                match bench_memory_profile(json) {
                    Ok(result) => {
                        if !json {
                            eprintln!(
                                " {}",
                                result.peak_rss.map(format_bytes).unwrap_or_default()
                            );
                        }
                        results.push(result);
                    }
                    Err(e) => {
                        if !json {
                            eprintln!(" FAILED: {e}");
                        }
                    }
                }
            }
            _ => {
                if !json {
                    eprint!("  Running {kind}...");
                }
                match run_benchmark(*kind, iterations) {
                    Ok(result) => {
                        if !json {
                            eprintln!(" {}", format_duration(result.median));
                        }
                        results.push(result);
                    }
                    Err(e) => {
                        if !json {
                            eprintln!(" FAILED: {e}");
                        }
                        // Continue with other benchmarks
                    }
                }
            }
        }
    }

    // Load comparison data
    let previous = if compare {
        load_previous_results(&config.storage.database_path)?
    } else {
        vec![]
    };

    let baselines = if ci {
        load_baseline_results(&config.storage.database_path)?
    } else {
        vec![]
    };

    // Save results
    save_results(
        &config.storage.database_path,
        &results,
        &system_info,
        baseline,
    )?;

    // Output results
    if json {
        let output = serde_json::json!({
            "system_info": system_info,
            "results": results.iter().map(|r| {
                serde_json::json!({
                    "benchmark": r.name,
                    "median_ns": r.median.as_nanos() as u64,
                    "min_ns": r.min.as_nanos() as u64,
                    "max_ns": r.max.as_nanos() as u64,
                    "peak_rss_bytes": r.peak_rss,
                    "iterations": r.iterations,
                })
            }).collect::<Vec<_>>(),
            "is_baseline": baseline,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        // Table output
        println!();
        println!(
            "  {:<16} {:<12} {:<12} {:<12} {:<12} Delta",
            "Benchmark", "Median", "Min", "Max", "Peak RSS"
        );
        println!("  {}", "-".repeat(76));

        for result in &results {
            let rss_str = result
                .peak_rss
                .map(format_bytes)
                .unwrap_or_else(|| "n/a".to_string());

            let delta_str = if compare {
                if let Some((_, prev_ns)) = previous.iter().find(|(name, _)| name == &result.name) {
                    let curr_ns = result.median.as_nanos() as i64;
                    let diff_pct = ((curr_ns - prev_ns) as f64 / *prev_ns as f64) * 100.0;
                    if diff_pct > 0.0 {
                        format!("+{diff_pct:.1}%")
                    } else {
                        format!("{diff_pct:.1}%")
                    }
                } else {
                    "new".to_string()
                }
            } else {
                String::new()
            };

            println!(
                "  {:<16} {:<12} {:<12} {:<12} {:<12} {}",
                result.name,
                format_duration(result.median),
                format_duration(result.min),
                format_duration(result.max),
                rss_str,
                delta_str,
            );
        }
        println!();
    }

    // CI mode: check for regressions
    if ci {
        let mut regressions = Vec::new();
        for result in &results {
            if let Some((_, baseline_ns)) = baselines.iter().find(|(name, _)| name == &result.name)
            {
                let curr_ns = result.median.as_nanos() as i64;
                let diff_pct = ((curr_ns - baseline_ns) as f64 / *baseline_ns as f64) * 100.0;
                if diff_pct > threshold_pct {
                    regressions.push((result.name.clone(), diff_pct));
                }
            }
        }

        if !regressions.is_empty() {
            eprintln!();
            eprintln!("  REGRESSION DETECTED (threshold: {threshold_pct:.0}%):");
            for (name, pct) in &regressions {
                eprintln!("    {name}: +{pct:.1}%");
            }
            eprintln!();
            std::process::exit(1);
        }
    }

    if baseline && !json {
        eprintln!("  Results saved as baseline.");
    }

    Ok(())
}
