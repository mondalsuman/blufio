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
            // Normalize first to show normalization results
            let normalized = blufio_injection::normalize::normalize(&text);
            let classifier =
                blufio_injection::classifier::InjectionClassifier::new(&config.injection_defense);
            let result = classifier.classify(&text, "user");

            // Build severity weights map for display
            let weights = &config.injection_defense.input_detection.severity_weights;

            if json {
                let matches_json: Vec<serde_json::Value> = result
                    .matches
                    .iter()
                    .map(|m| {
                        let weight = weights
                            .get(&m.category.to_string())
                            .copied()
                            .unwrap_or(1.0)
                            .clamp(0.0, 3.0);
                        let pattern = blufio_injection::patterns::PATTERNS.get(m.pattern_index);
                        let language = pattern.map(|p| p.language).unwrap_or("en");
                        serde_json::json!({
                            "category": m.category.to_string(),
                            "language": language,
                            "severity": m.severity,
                            "weighted_severity": m.severity * weight,
                            "weight": weight,
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
                    "normalization": {
                        "normalized_text": normalized.text,
                        "zero_width_count": normalized.report.zero_width_count,
                        "confusables_mapped": normalized.report.confusables_mapped,
                        "base64_segments_decoded": normalized.report.base64_segments_decoded,
                        "text_changed": normalized.text != text,
                    },
                    "severity_weights": weights,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                let use_color = !plain && std::io::stdout().is_terminal();
                println!();
                println!("  Injection Defense Test");
                println!("  {}", "-".repeat(50));
                println!("  Input: \"{}\"", text);

                // Normalization section
                if normalized.text != text
                    || normalized.report.zero_width_count > 0
                    || normalized.report.confusables_mapped > 0
                    || normalized.report.base64_segments_decoded > 0
                {
                    println!();
                    println!("  Normalization:");
                    if normalized.text != text {
                        println!("    Normalized: \"{}\"", normalized.text);
                    }
                    if normalized.report.zero_width_count > 0 {
                        println!(
                            "    Zero-width chars stripped: {}",
                            normalized.report.zero_width_count
                        );
                    }
                    if normalized.report.confusables_mapped > 0 {
                        println!(
                            "    Confusable chars mapped:   {}",
                            normalized.report.confusables_mapped
                        );
                    }
                    if normalized.report.base64_segments_decoded > 0 {
                        println!(
                            "    Base64 segments decoded:   {}",
                            normalized.report.base64_segments_decoded
                        );
                    }
                }

                println!();
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
                        let weight = weights
                            .get(&mp.category.to_string())
                            .copied()
                            .unwrap_or(1.0)
                            .clamp(0.0, 3.0);
                        let weighted_severity = mp.severity * weight;
                        let pattern = blufio_injection::patterns::PATTERNS.get(mp.pattern_index);
                        let language = pattern.map(|p| p.language).unwrap_or("en");
                        println!(
                            "    - {} [{}] (base: {:.2}, weight: {:.1}, weighted: {:.2}, text: \"{}\")",
                            mp.category,
                            language,
                            mp.severity,
                            weight,
                            weighted_severity,
                            mp.matched_text
                        );
                    }
                }

                // Show effective severity weights if any are non-default
                if !weights.is_empty() {
                    println!();
                    println!("  Severity weights:");
                    for (cat, w) in weights {
                        println!("    {}: {:.1}", cat, w);
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

            // Count patterns per language and per category
            let mut language_counts = std::collections::HashMap::new();
            let mut category_counts = std::collections::HashMap::new();
            for p in blufio_injection::patterns::PATTERNS.iter() {
                *language_counts.entry(p.language).or_insert(0usize) += 1;
                *category_counts
                    .entry(p.category.to_string())
                    .or_insert(0usize) += 1;
            }

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
                    "pattern_count": blufio_injection::patterns::PATTERNS.len(),
                    "patterns_per_language": language_counts,
                    "patterns_per_category": category_counts,
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
                println!(
                    "    Built-in patterns: {}",
                    blufio_injection::patterns::PATTERNS.len()
                );
                println!();
                println!("  Pattern languages:");
                let mut langs: Vec<_> = language_counts.iter().collect();
                langs.sort_by_key(|(lang, _)| *lang);
                for (lang, count) in &langs {
                    println!("    {}: {} patterns", lang, count);
                }
                println!();
                println!("  Pattern categories:");
                let mut cats: Vec<_> = category_counts.iter().collect();
                cats.sort_by_key(|(cat, _)| (*cat).clone());
                for (cat, count) in &cats {
                    println!("    {}: {} patterns", cat, count);
                }
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
                let mut output = serde_json::to_value(&config.injection_defense)
                    .unwrap_or(serde_json::json!({}));
                // Add effective severity weights (defaults for all categories)
                let default_categories = [
                    "role_hijacking",
                    "instruction_override",
                    "data_exfiltration",
                    "prompt_leaking",
                    "jailbreak",
                    "delimiter_manipulation",
                    "indirect_injection",
                    "encoding_evasion",
                ];
                let weights = &config.injection_defense.input_detection.severity_weights;
                let mut effective_weights = serde_json::Map::new();
                for cat in &default_categories {
                    let w = weights.get(*cat).copied().unwrap_or(1.0);
                    effective_weights.insert(cat.to_string(), serde_json::Value::from(w));
                }
                if let Some(obj) = output.as_object_mut() {
                    obj.insert(
                        "effective_severity_weights".to_string(),
                        serde_json::Value::Object(effective_weights),
                    );
                }
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                let toml_str = toml::to_string_pretty(&config.injection_defense)
                    .unwrap_or_else(|_| "[serialization error]".to_string());
                println!();
                println!("  Effective Injection Defense Config");
                println!("  {}", "-".repeat(50));
                println!("{}", toml_str);

                // Show effective severity weights
                let weights = &config.injection_defense.input_detection.severity_weights;
                let default_categories = [
                    "role_hijacking",
                    "instruction_override",
                    "data_exfiltration",
                    "prompt_leaking",
                    "jailbreak",
                    "delimiter_manipulation",
                    "indirect_injection",
                    "encoding_evasion",
                ];
                println!("  Effective Severity Weights:");
                for cat in &default_categories {
                    let w = weights.get(*cat).copied().unwrap_or(1.0);
                    println!("    {}: {:.1}", cat, w);
                }
                println!();
            }
        }
        InjectionCommands::TestCanary => {
            let pass = blufio_injection::canary::CanaryTokenManager::self_test();
            if pass {
                use std::io::IsTerminal;
                if std::io::stdout().is_terminal() {
                    use colored::Colorize;
                    println!("  Canary self-test: {}", "PASS".green());
                } else {
                    println!("  Canary self-test: PASS");
                }
            } else {
                use std::io::IsTerminal;
                if std::io::stdout().is_terminal() {
                    use colored::Colorize;
                    println!("  Canary self-test: {}", "FAIL".red());
                } else {
                    println!("  Canary self-test: FAIL");
                }
                std::process::exit(1);
            }
        }
        InjectionCommands::ValidateCorpus { path, json } => {
            // Read corpus file
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("error: cannot read corpus file '{}': {}", path, e);
                    std::process::exit(1);
                }
            };

            // Parse as JSON array of strings
            let messages: Vec<String> = match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: corpus file must be a JSON array of strings: {}", e);
                    std::process::exit(1);
                }
            };

            let classifier =
                blufio_injection::classifier::InjectionClassifier::new(&config.injection_defense);

            let mut false_positives = Vec::new();
            for (i, msg) in messages.iter().enumerate() {
                let result = classifier.classify(msg, "user");
                if result.score > 0.0 {
                    false_positives.push(serde_json::json!({
                        "index": i,
                        "message": msg,
                        "score": result.score,
                        "categories": result.categories,
                    }));
                }
            }

            let total = messages.len();
            let clean = total - false_positives.len();
            let fp_count = false_positives.len();

            if json {
                let output = serde_json::json!({
                    "total": total,
                    "clean": clean,
                    "false_positives": fp_count,
                    "details": false_positives,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                println!();
                if fp_count == 0 {
                    println!("  {}/{} messages clean (0 false positives)", clean, total);
                } else {
                    println!("  {} false positive(s) detected:", fp_count);
                    for fp in &false_positives {
                        println!(
                            "    [{}] score={:.4} categories={} text=\"{}\"",
                            fp["index"],
                            fp["score"].as_f64().unwrap_or(0.0),
                            fp["categories"],
                            fp["message"].as_str().unwrap_or(""),
                        );
                    }
                    println!();
                    println!(
                        "  Summary: {}/{} clean, {} false positive(s)",
                        clean, total, fp_count
                    );
                }
                println!();
            }

            if fp_count > 0 {
                std::process::exit(1);
            }
        }
    }
}
