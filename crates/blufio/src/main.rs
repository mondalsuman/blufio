// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Blufio - An always-on personal AI agent.
//!
//! This is the binary entry point for the Blufio agent.

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

use clap::{Parser, Subcommand};

/// Blufio - An always-on personal AI agent.
#[derive(Parser, Debug)]
#[command(name = "blufio", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Available subcommands.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the Blufio agent server.
    Serve,
    /// Launch an interactive REPL session.
    Shell,
    /// Manage Blufio configuration.
    Config,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve => {
            println!("blufio serve: not yet implemented");
        }
        Commands::Shell => {
            println!("blufio shell: not yet implemented");
        }
        Commands::Config => {
            println!("blufio config: not yet implemented");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(not(target_env = "msvc"))]
    fn jemalloc_is_active() {
        // Verify jemalloc is the global allocator by advancing the epoch.
        // Only jemalloc supports this -- the system allocator would fail.
        use tikv_jemalloc_ctl::{epoch, stats};
        epoch::advance().unwrap();
        let allocated = stats::allocated::read().unwrap();
        assert!(allocated > 0, "jemalloc should report non-zero allocation");
    }
}
