// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Memory management CLI handlers for `blufio memory` subcommands.

use crate::MemoryCommand;

/// Handle `blufio memory <command>` subcommands.
pub(crate) async fn handle_memory_command(
    config: &blufio_config::model::BlufioConfig,
    command: MemoryCommand,
) -> Result<(), blufio_core::BlufioError> {
    match command {
        MemoryCommand::Validate { dry_run, json } => {
            let conn = blufio_storage::open_connection(&config.storage.database_path).await?;
            let store = blufio_memory::MemoryStore::new(conn);

            if dry_run {
                let memories = store.get_all_active_with_embeddings().await?;
                let result =
                    blufio_memory::validation::run_validation_dry_run(&memories, &config.memory);

                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "dry_run": true,
                            "duplicates": result.duplicates_found,
                            "conflicts": result.conflicts_found,
                            "stale": result.stale_found,
                        })
                    );
                } else {
                    println!("Validation (dry run):");
                    println!("  Duplicates: {}", result.duplicates_found);
                    println!("  Conflicts:  {}", result.conflicts_found);
                    println!("  Stale:      {}", result.stale_found);
                }
            } else {
                let result =
                    blufio_memory::validation::run_validation(&store, &config.memory, &None)
                        .await?;

                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "dry_run": false,
                            "duplicates": result.duplicates_found,
                            "conflicts": result.conflicts_found,
                            "stale": result.stale_found,
                        })
                    );
                } else {
                    println!("Validation complete:");
                    println!("  Duplicates resolved: {}", result.duplicates_found);
                    println!("  Conflicts resolved:  {}", result.conflicts_found);
                    println!("  Stale removed:       {}", result.stale_found);
                }
            }
        }
    }
    Ok(())
}
