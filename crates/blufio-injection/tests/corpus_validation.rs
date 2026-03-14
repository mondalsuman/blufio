// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Corpus validation integration tests.
//!
//! These are HARD CI GATES:
//! - benign_corpus.json: 0% false positive tolerance (no message may score > 0)
//! - attack_corpus.json: 100% detection rate (every message must score > 0)

use blufio_injection::classifier::InjectionClassifier;
use blufio_injection::config::InjectionDefenseConfig;

fn load_corpus(path: &str) -> Vec<String> {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("Failed to read corpus file {}: {}", path, e));
    serde_json::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse corpus file {}: {}", path, e))
}

#[test]
fn test_benign_corpus_zero_false_positives() {
    let corpus = load_corpus("tests/fixtures/benign_corpus.json");
    assert!(
        corpus.len() >= 100,
        "Benign corpus must have 100+ messages, got {}",
        corpus.len()
    );

    let config = InjectionDefenseConfig::default();
    let classifier = InjectionClassifier::new(&config);

    let mut failures: Vec<(usize, String, f64)> = Vec::new();

    for (i, message) in corpus.iter().enumerate() {
        let result = classifier.classify(message, "user");
        if result.score > 0.0 {
            failures.push((i, message.clone(), result.score));
        }
    }

    assert!(
        failures.is_empty(),
        "FALSE POSITIVES DETECTED ({}/{}): {}",
        failures.len(),
        corpus.len(),
        failures
            .iter()
            .map(|(i, msg, score)| format!(
                "\n  [{}] score={:.4}: \"{}\"",
                i,
                score,
                if msg.len() > 80 { &msg[..80] } else { msg }
            ))
            .collect::<String>()
    );
}

#[test]
fn test_attack_corpus_all_detected() {
    let corpus = load_corpus("tests/fixtures/attack_corpus.json");
    assert!(
        corpus.len() >= 50,
        "Attack corpus must have 50+ messages, got {}",
        corpus.len()
    );

    let config = InjectionDefenseConfig::default();
    let classifier = InjectionClassifier::new(&config);

    let mut misses: Vec<(usize, String)> = Vec::new();

    for (i, message) in corpus.iter().enumerate() {
        let result = classifier.classify(message, "user");
        if result.score == 0.0 {
            misses.push((i, message.clone()));
        }
    }

    assert!(
        misses.is_empty(),
        "MISSED ATTACKS ({}/{}): {}",
        misses.len(),
        corpus.len(),
        misses
            .iter()
            .map(|(i, msg)| format!(
                "\n  [{}]: \"{}\"",
                i,
                if msg.len() > 80 { &msg[..80] } else { msg }
            ))
            .collect::<String>()
    );
}
