// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Plugin management CLI handlers for `blufio plugin` subcommands.

use crate::PluginCommands;

/// Handle `blufio plugin <action>` subcommands.
pub(crate) fn handle_plugin_command(
    config: &blufio_config::model::BlufioConfig,
    action: PluginCommands,
) -> Result<(), blufio_core::BlufioError> {
    match action {
        PluginCommands::List => {
            let catalog = blufio_plugin::builtin_catalog();
            let mut registry = blufio_plugin::PluginRegistry::new();

            for manifest in catalog {
                // Determine status based on config overrides and required config keys.
                let name = manifest.name.clone();
                let config_override = config.plugin.plugins.get(&name);

                let status = match config_override {
                    Some(false) => blufio_plugin::PluginStatus::Disabled,
                    Some(true) => blufio_plugin::PluginStatus::Enabled,
                    None => {
                        // Check if required config keys are present.
                        let all_configured = manifest
                            .config_keys
                            .iter()
                            .all(|key| is_config_key_present(config, key));
                        if all_configured || manifest.config_keys.is_empty() {
                            blufio_plugin::PluginStatus::Enabled
                        } else {
                            blufio_plugin::PluginStatus::NotConfigured
                        }
                    }
                };

                registry.register_with_status(manifest, None, status);
            }

            println!("{:<18} {:<15} {:<16} DESCRIPTION", "NAME", "TYPE", "STATUS");
            println!("{}", "-".repeat(75));
            for entry in registry.list_all() {
                println!(
                    "{:<18} {:<15} {:<16} {}",
                    entry.manifest.name,
                    entry.manifest.adapter_type.to_string(),
                    entry.status,
                    entry.manifest.description,
                );
            }
            Ok(())
        }
        PluginCommands::Search { query } => {
            let results = blufio_plugin::search_catalog(&query);
            if results.is_empty() {
                println!("No plugins found matching '{query}'.");
            } else {
                println!("{:<18} {:<15} DESCRIPTION", "NAME", "TYPE");
                println!("{}", "-".repeat(65));
                for manifest in &results {
                    println!(
                        "{:<18} {:<15} {}",
                        manifest.name,
                        manifest.adapter_type.to_string(),
                        manifest.description,
                    );
                }
            }
            Ok(())
        }
        PluginCommands::Install { name } => {
            let catalog = blufio_plugin::builtin_catalog();
            let found = catalog.iter().find(|m| m.name == name);

            match found {
                Some(manifest) => {
                    println!("Plugin '{}' enabled.", name);
                    if !manifest.config_keys.is_empty() {
                        println!(
                            "  Required config keys: {}",
                            manifest.config_keys.join(", ")
                        );
                        println!("  Add configuration to blufio.toml if required.");
                    }
                    Ok(())
                }
                None => Err(blufio_core::BlufioError::AdapterNotFound {
                    adapter_type: "plugin".to_string(),
                    name,
                }),
            }
        }
        PluginCommands::Remove { name } => {
            let catalog = blufio_plugin::builtin_catalog();
            let found = catalog.iter().any(|m| m.name == name);

            if found {
                println!("Plugin '{name}' disabled.");
                Ok(())
            } else {
                Err(blufio_core::BlufioError::AdapterNotFound {
                    adapter_type: "plugin".to_string(),
                    name,
                })
            }
        }
        PluginCommands::Update => {
            println!("Plugins are compiled into the Blufio binary.");
            println!("Update by rebuilding or downloading a new binary release.");
            Ok(())
        }
    }
}

/// Check if a config key is present (non-empty) in the loaded config.
///
/// Supports dotted key paths like "telegram.bot_token" and "anthropic.api_key".
pub(crate) fn is_config_key_present(
    config: &blufio_config::model::BlufioConfig,
    key: &str,
) -> bool {
    match key {
        "telegram.bot_token" => config.telegram.bot_token.is_some(),
        "anthropic.api_key" => config.anthropic.api_key.is_some(),
        _ => false,
    }
}
