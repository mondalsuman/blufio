CREATE TABLE IF NOT EXISTS bench_results (
    id INTEGER PRIMARY KEY,
    benchmark TEXT NOT NULL,
    median_ns INTEGER NOT NULL,
    min_ns INTEGER NOT NULL,
    max_ns INTEGER NOT NULL,
    peak_rss_bytes INTEGER,
    iterations INTEGER NOT NULL,
    system_info TEXT NOT NULL,
    is_baseline INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX idx_bench_results_benchmark ON bench_results(benchmark, created_at);
