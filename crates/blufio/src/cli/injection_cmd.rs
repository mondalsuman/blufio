// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Injection defense CLI handlers for `blufio injection` subcommands.

use crate::InjectionCommands;

/// Handle injection defense CLI subcommands.
pub(crate) fn run_injection_command(
    config: &blufio_config::model::BlufioConfig,
    action: InjectionCommands,
) {
    use std::io::IsTerminal;

    match action {
        InjectionCommands::Test { text, json, plain } => {
            let classifier =
                blufio_injection::classifier::InjectionClassifier::new(&config.injection_defense);
            let result = classifier.classify(&text, "user");

            if json {
                let matches_json: Vec<serde_json::Value> = result
                    .matches
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "category": format!("{:?}", m.category),
                            "severity": m.severity,
                            "matched_text": m.matched_text,
                            "span": [m.span.start, m.span.end],
                        })
                    })
                    .collect();
                let output = serde_json::json!({
                    "text": text,
                    "score": result.score,
                    "action": result.action,
                    "categories": result.categories,
                    "matches": matches_json,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                let use_color = !plain && std::io::stdout().is_terminal();
                println!();
                println!("  Injection Defense Test");
                println!("  {}", "-".repeat(50));
                println!("  Input: \"{}\"", text);
                println!("  Score: {:.4}", result.score);

                let action_display = if use_color {
                    use colored::Colorize;
                    match result.action.as_str() {
                        "clean" => result.action.green().to_string(),
                        "logged" => result.action.yellow().to_string(),
                        "blocked" => result.action.red().to_string(),
                        "dry_run" => result.action.cyan().to_string(),
                        other => other.to_string(),
                    }
                } else {
                    result.action.clone()
                };
                println!("  Action: {}", action_display);

                if !result.categories.is_empty() {
                    println!("  Categories: {}", result.categories.join(", "));
                }
                if !result.matches.is_empty() {
                    println!("  Matched patterns:");
                    for mp in &result.matches {
                        println!(
                            "    - {:?} (severity: {:.2}, text: \"{}\")",
                            mp.category, mp.severity, mp.matched_text
                        );
                    }
                }
                println!();
            }
        }
        InjectionCommands::Status { json } => {
            let cfg = &config.injection_defense;
            let active_layers: Vec<&str> = [
                Some("L1 (input detection)"),
                if cfg.hmac_boundaries.enabled {
                    Some("L3 (HMAC boundaries)")
                } else {
                    None
                },
                if cfg.output_screening.enabled {
                    Some("L4 (output screening)")
                } else {
                    None
                },
                if cfg.hitl.enabled {
                    Some("L5 (human-in-the-loop)")
                } else {
                    None
                },
            ]
            .iter()
            .filter_map(|l| *l)
            .collect();

            if json {
                let output = serde_json::json!({
                    "enabled": cfg.enabled,
                    "dry_run": cfg.dry_run,
                    "active_layers": active_layers,
                    "layer_count": active_layers.len(),
                    "input_detection_mode": cfg.input_detection.mode,
                    "blocking_threshold": cfg.input_detection.blocking_threshold,
                    "mcp_blocking_threshold": cfg.input_detection.mcp_blocking_threshold,
                    "custom_patterns": cfg.input_detection.custom_patterns.len(),
                    "output_screening_enabled": cfg.output_screening.enabled,
                    "escalation_threshold": cfg.output_screening.escalation_threshold,
                    "hitl_enabled": cfg.hitl.enabled,
                    "hitl_timeout_secs": cfg.hitl.timeout_secs,
                    "hmac_boundaries_enabled": cfg.hmac_boundaries.enabled,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                println!();
                println!("  Injection Defense Status");
                println!("  {}", "-".repeat(50));
                println!(
                    "  Enabled:        {}",
                    if cfg.enabled { "yes" } else { "no" }
                );
                println!(
                    "  Dry run:        {}",
                    if cfg.dry_run { "yes" } else { "no" }
                );
                println!("  Active layers:  {}", active_layers.join(", "));
                println!();
                println!("  L1 Input Detection:");
                println!("    Mode:              {}", cfg.input_detection.mode);
                println!(
                    "    Blocking threshold: {:.2}",
                    cfg.input_detection.blocking_threshold
                );
                println!(
                    "    MCP threshold:     {:.2}",
                    cfg.input_detection.mcp_blocking_threshold
                );
                println!(
                    "    Custom patterns:   {}",
                    cfg.input_detection.custom_patterns.len()
                );
                println!();
                println!("  L3 HMAC Boundaries:");
                println!(
                    "    Enabled:           {}",
                    if cfg.hmac_boundaries.enabled {
                        "yes"
                    } else {
                        "no"
                    }
                );
                println!();
                println!("  L4 Output Screening:");
                println!(
                    "    Enabled:           {}",
                    if cfg.output_screening.enabled {
                        "yes"
                    } else {
                        "no"
                    }
                );
                println!(
                    "    Escalation after:  {} failures",
                    cfg.output_screening.escalation_threshold
                );
                println!();
                println!("  L5 Human-in-the-Loop:");
                println!(
                    "    Enabled:           {}",
                    if cfg.hitl.enabled { "yes" } else { "no" }
                );
                println!("    Timeout:           {}s", cfg.hitl.timeout_secs);
                println!();
            }
        }
        InjectionCommands::Config { json } => {
            if json {
                let output = serde_json::to_value(&config.injection_defense)
                    .unwrap_or(serde_json::json!({}));
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                let toml_str = toml::to_string_pretty(&config.injection_defense)
                    .unwrap_or_else(|_| "[serialization error]".to_string());
                println!();
                println!("  Effective Injection Defense Config");
                println!("  {}", "-".repeat(50));
                println!("{}", toml_str);
            }
        }
    }
}
