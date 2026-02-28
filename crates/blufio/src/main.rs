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
    command: Option<Commands>,
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

    // Load and validate configuration at startup
    let _config = match blufio_config::load_and_validate() {
        Ok(config) => {
            eprintln!(
                "blufio: config loaded (agent.name={})",
                config.agent.name
            );
            config
        }
        Err(errors) => {
            blufio_config::render_errors(&errors);
            std::process::exit(1);
        }
    };

    match cli.command {
        Some(Commands::Serve) => {
            println!("blufio serve: not yet implemented");
        }
        Some(Commands::Shell) => {
            println!("blufio shell: not yet implemented");
        }
        Some(Commands::Config) => {
            println!("blufio config: not yet implemented");
        }
        None => {
            println!("blufio: use --help for available commands");
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

    #[test]
    fn binary_loads_config_defaults() {
        // Verify config loads with defaults (no config file needed)
        let config = blufio_config::load_and_validate()
            .expect("default config should be valid");
        assert_eq!(config.agent.name, "blufio");
    }
}
