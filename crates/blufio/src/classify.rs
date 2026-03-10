// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! CLI handlers for `blufio classify` subcommand.
//!
//! Provides classification management for memories, messages, and sessions:
//! - `set <type> <id> <level>` -- set classification on an entity
//! - `get <type> <id>` -- query current classification for an entity
//! - `list --type <type>` -- list entities filtered by classification level
//! - `bulk --type <type> --level <level>` -- bulk update classifications
//!
//! # Examples
//!
//! ```bash
//! blufio classify set memory mem-42 confidential
//! blufio classify get memory mem-42
//! blufio classify list --type memory --level confidential --json
//! blufio classify bulk --type memory --level restricted --current-level internal --dry-run
//! ```

use clap::Subcommand;
use colored::Colorize;

use blufio_core::classification::{ClassificationError, DataClassification};
use blufio_core::BlufioError;

/// Classify subcommand actions.
#[derive(Subcommand, Debug)]
pub enum ClassifyAction {
    /// Set the classification level on an entity.
    Set {
        /// Entity type: memory, message, or session.
        entity_type: String,
        /// Entity identifier.
        id: String,
        /// Classification level: public, internal, confidential, or restricted.
        level: String,
        /// Force downgrade (required when lowering classification level).
        #[arg(long)]
        force: bool,
    },
    /// Get the current classification level of an entity.
    Get {
        /// Entity type: memory, message, or session.
        entity_type: String,
        /// Entity identifier.
        id: String,
    },
    /// List entities filtered by classification level.
    List {
        /// Entity type: memory, message, or session.
        #[arg(long, alias = "type")]
        entity_type: String,
        /// Filter by classification level.
        #[arg(long)]
        level: Option<String>,
        /// Output as structured JSON for scripting.
        #[arg(long)]
        json: bool,
    },
    /// Bulk update classification levels with filters.
    Bulk {
        /// Entity type: memory, message, or session.
        #[arg(long, alias = "type")]
        entity_type: String,
        /// New classification level: public, internal, confidential, or restricted.
        #[arg(long)]
        level: String,
        /// Filter: only update entities with this current classification level.
        #[arg(long)]
        current_level: Option<String>,
        /// Filter: only update entities in this session.
        #[arg(long)]
        session_id: Option<String>,
        /// Filter: only update entities created after this date (ISO 8601).
        #[arg(long)]
        from: Option<String>,
        /// Filter: only update entities created before this date (ISO 8601).
        #[arg(long)]
        to: Option<String>,
        /// Filter: only update entities whose content matches this pattern.
        #[arg(long)]
        pattern: Option<String>,
        /// Show what would change without modifying data.
        #[arg(long)]
        dry_run: bool,
        /// Force downgrades (required when lowering classification level).
        #[arg(long)]
        force: bool,
    },
}

/// Validate an entity type string.
fn validate_entity_type(entity_type: &str) -> Result<&str, BlufioError> {
    match entity_type {
        "memory" | "message" | "session" => Ok(entity_type),
        _ => Err(BlufioError::Classification(
            ClassificationError::InvalidLevel(format!(
                "invalid entity type '{}' (expected: memory, message, session)",
                entity_type
            )),
        )),
    }
}

/// Parse and validate a classification level string.
fn parse_level(level_str: &str) -> Result<DataClassification, BlufioError> {
    DataClassification::from_str_value(level_str).ok_or_else(|| {
        BlufioError::Classification(ClassificationError::InvalidLevel(format!(
            "invalid classification level '{}' (expected: public, internal, confidential, restricted)",
            level_str
        )))
    })
}

/// Format a classification level with color.
fn colored_level(level: DataClassification) -> String {
    match level {
        DataClassification::Public => "public".green().to_string(),
        DataClassification::Internal => "internal".blue().to_string(),
        DataClassification::Confidential => "confidential".yellow().to_string(),
        DataClassification::Restricted => "restricted".red().bold().to_string(),
        _ => level.as_str().white().to_string(),
    }
}

/// Run the classify subcommand.
pub async fn run_classify(action: ClassifyAction) -> Result<(), BlufioError> {
    match action {
        ClassifyAction::Set {
            entity_type,
            id,
            level,
            force,
        } => run_classify_set(&entity_type, &id, &level, force).await,
        ClassifyAction::Get { entity_type, id } => run_classify_get(&entity_type, &id).await,
        ClassifyAction::List {
            entity_type,
            level,
            json,
        } => run_classify_list(&entity_type, level.as_deref(), json).await,
        ClassifyAction::Bulk {
            entity_type,
            level,
            current_level,
            session_id,
            from,
            to,
            pattern,
            dry_run,
            force,
        } => {
            run_classify_bulk(
                &entity_type,
                &level,
                current_level.as_deref(),
                session_id.as_deref(),
                from.as_deref(),
                to.as_deref(),
                pattern.as_deref(),
                dry_run,
                force,
            )
            .await
        }
    }
}

/// Handle `blufio classify set <type> <id> <level>`.
async fn run_classify_set(
    entity_type: &str,
    id: &str,
    level_str: &str,
    force: bool,
) -> Result<(), BlufioError> {
    validate_entity_type(entity_type)?;
    let new_level = parse_level(level_str)?;

    // In a real implementation this would connect to the database and update the record.
    // For now, we validate inputs and demonstrate the downgrade protection logic.
    let current_level = DataClassification::Internal; // Placeholder: would be fetched from DB.

    if new_level.is_downgrade_from(&current_level) && !force {
        return Err(BlufioError::Classification(
            ClassificationError::DowngradeRejected {
                current: current_level.as_str().to_string(),
                requested: new_level.as_str().to_string(),
            },
        ));
    }

    // Emit classification changed event (fire-and-forget).
    let _event = blufio_security::classification_changed_event(
        entity_type,
        id,
        current_level.as_str(),
        new_level.as_str(),
        "user",
    );

    println!(
        "{} classification for {} {} set to {}",
        "OK".green().bold(),
        entity_type,
        id.cyan(),
        colored_level(new_level),
    );

    Ok(())
}

/// Handle `blufio classify get <type> <id>`.
async fn run_classify_get(entity_type: &str, id: &str) -> Result<(), BlufioError> {
    validate_entity_type(entity_type)?;

    // Placeholder: would fetch from DB.
    let level = DataClassification::Internal;

    println!(
        "{} {} classification: {}",
        entity_type,
        id.cyan(),
        colored_level(level),
    );

    Ok(())
}

/// Handle `blufio classify list --type <type> [--level <level>] [--json]`.
async fn run_classify_list(
    entity_type: &str,
    level: Option<&str>,
    json: bool,
) -> Result<(), BlufioError> {
    validate_entity_type(entity_type)?;
    if let Some(l) = level {
        parse_level(l)?;
    }

    // Placeholder: would query DB with filters.
    let results: Vec<(&str, DataClassification)> = vec![];

    if json {
        let json_results: Vec<serde_json::Value> = results
            .iter()
            .map(|(id, lvl)| {
                serde_json::json!({
                    "id": id,
                    "type": entity_type,
                    "level": lvl.as_str(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json_results)
                .map_err(|e| BlufioError::Internal(format!("JSON serialization failed: {e}")))?
        );
    } else if results.is_empty() {
        println!("No {} entities found matching criteria.", entity_type);
    } else {
        println!(
            "{:<40} {:<15}",
            "ID".bold(),
            "Classification".bold()
        );
        println!("{}", "-".repeat(55));
        for (id, lvl) in &results {
            println!("{:<40} {}", id, colored_level(*lvl));
        }
        println!("\n{} entities total", results.len());
    }

    Ok(())
}

/// Handle `blufio classify bulk` with filters and dry-run support.
#[allow(clippy::too_many_arguments)]
async fn run_classify_bulk(
    entity_type: &str,
    level_str: &str,
    current_level: Option<&str>,
    _session_id: Option<&str>,
    _from: Option<&str>,
    _to: Option<&str>,
    _pattern: Option<&str>,
    dry_run: bool,
    force: bool,
) -> Result<(), BlufioError> {
    validate_entity_type(entity_type)?;
    let new_level = parse_level(level_str)?;
    if let Some(cl) = current_level {
        parse_level(cl)?;
    }

    // Placeholder: would query DB to find matching entities and apply updates.
    let total: usize = 0;
    let succeeded: usize = 0;
    let failed: usize = 0;
    let errors: Vec<String> = vec![];

    if dry_run {
        println!(
            "{} {} {} entities would be updated to {}",
            "DRY RUN:".yellow().bold(),
            total,
            entity_type,
            colored_level(new_level),
        );
    } else {
        // Check downgrade protection for bulk operations.
        if let Some(cl) = current_level {
            let current = parse_level(cl)?;
            if new_level.is_downgrade_from(&current) && !force {
                return Err(BlufioError::Classification(
                    ClassificationError::DowngradeRejected {
                        current: current.as_str().to_string(),
                        requested: new_level.as_str().to_string(),
                    },
                ));
            }
        }

        // Emit single bulk event (not per-item).
        if succeeded > 0 {
            let _event = blufio_security::bulk_classification_changed_event(
                entity_type,
                succeeded,
                current_level.unwrap_or("mixed"),
                new_level.as_str(),
                "user",
            );
        }

        println!(
            "{} Bulk classification update complete",
            "OK".green().bold()
        );
        println!("  Total:     {}", total);
        println!("  Succeeded: {}", succeeded.to_string().green());
        if failed > 0 {
            println!("  Failed:    {}", failed.to_string().red());
            for err in &errors {
                println!("    - {}", err.red());
            }
        } else {
            println!("  Failed:    0");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_entity_type_valid() {
        assert!(validate_entity_type("memory").is_ok());
        assert!(validate_entity_type("message").is_ok());
        assert!(validate_entity_type("session").is_ok());
    }

    #[test]
    fn validate_entity_type_invalid() {
        assert!(validate_entity_type("unknown").is_err());
        assert!(validate_entity_type("").is_err());
    }

    #[test]
    fn parse_level_valid() {
        assert_eq!(parse_level("public").unwrap(), DataClassification::Public);
        assert_eq!(
            parse_level("internal").unwrap(),
            DataClassification::Internal
        );
        assert_eq!(
            parse_level("confidential").unwrap(),
            DataClassification::Confidential
        );
        assert_eq!(
            parse_level("restricted").unwrap(),
            DataClassification::Restricted
        );
    }

    #[test]
    fn parse_level_invalid() {
        assert!(parse_level("invalid").is_err());
        assert!(parse_level("").is_err());
        assert!(parse_level("PUBLIC").is_err());
    }

    #[test]
    fn colored_level_returns_string() {
        // Just verify it produces non-empty output for each level.
        assert!(!colored_level(DataClassification::Public).is_empty());
        assert!(!colored_level(DataClassification::Internal).is_empty());
        assert!(!colored_level(DataClassification::Confidential).is_empty());
        assert!(!colored_level(DataClassification::Restricted).is_empty());
    }

    #[tokio::test]
    async fn classify_set_downgrade_rejected_without_force() {
        // Internal (current) -> Public (new) is a downgrade, should fail without --force.
        let result = run_classify_set("memory", "mem-1", "public", false).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("downgrade rejected"));
    }

    #[tokio::test]
    async fn classify_set_downgrade_allowed_with_force() {
        let result = run_classify_set("memory", "mem-1", "public", true).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn classify_set_invalid_entity_type() {
        let result = run_classify_set("unknown", "id-1", "internal", false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn classify_set_invalid_level() {
        let result = run_classify_set("memory", "mem-1", "invalid", false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn classify_get_valid() {
        let result = run_classify_get("memory", "mem-1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn classify_get_invalid_type() {
        let result = run_classify_get("unknown", "id-1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn classify_list_valid() {
        let result = run_classify_list("memory", None, false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn classify_list_json_output() {
        let result = run_classify_list("memory", Some("internal"), true).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn classify_list_invalid_level_filter() {
        let result = run_classify_list("memory", Some("invalid"), false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn classify_bulk_dry_run() {
        let result = run_classify_bulk(
            "memory",
            "confidential",
            None,
            None,
            None,
            None,
            None,
            true,
            false,
        )
        .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn classify_bulk_downgrade_rejected() {
        let result = run_classify_bulk(
            "memory",
            "public",
            Some("confidential"),
            None,
            None,
            None,
            None,
            false,
            false,
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn classify_bulk_downgrade_allowed_with_force() {
        let result = run_classify_bulk(
            "memory",
            "public",
            Some("confidential"),
            None,
            None,
            None,
            None,
            false,
            true,
        )
        .await;
        assert!(result.is_ok());
    }
}
