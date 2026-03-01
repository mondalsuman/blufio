// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Heuristic query complexity classification.
//!
//! Classifies user messages into Simple/Standard/Complex tiers using
//! zero-cost heuristic rules. No LLM pre-call, no network, no latency.

/// Query complexity tiers mapped to Claude model families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplexityTier {
    /// Haiku: greetings, time queries, single-fact lookups, yes/no.
    Simple,
    /// Sonnet: general conversation, moderate analysis, Q&A.
    Standard,
    /// Opus: multi-step reasoning, code generation, detailed analysis.
    Complex,
}

impl std::fmt::Display for ComplexityTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComplexityTier::Simple => write!(f, "simple"),
            ComplexityTier::Standard => write!(f, "standard"),
            ComplexityTier::Complex => write!(f, "complex"),
        }
    }
}

/// Result of classifying a query's complexity.
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// The classified complexity tier.
    pub tier: ComplexityTier,
    /// Confidence in the classification (0.0-1.0).
    pub confidence: f32,
    /// Human-readable reason for the classification.
    pub reason: &'static str,
}

/// Simple greeting/farewell patterns (exact match, case-insensitive).
const SIMPLE_EXACT: &[&str] = &[
    "hi", "hello", "hey", "thanks", "thank you", "bye", "ok", "okay",
    "yes", "no", "sure", "good", "great", "cool", "nice", "wow", "lol",
    "haha", "yep", "nope", "yea", "yeah", "nah",
];

/// Simple question patterns (contains, case-insensitive).
const SIMPLE_QUESTIONS: &[&str] = &[
    "what time", "what day", "what date", "how are you",
    "what's up", "who are you", "what's your name",
    "what is the time", "what is the date",
];

/// Complex indicator patterns (contains, case-insensitive).
const COMPLEX_INDICATORS: &[&str] = &[
    "analyze", "compare", "evaluate", "implement", "design",
    "architecture", "trade-off", "tradeoff", "pros and cons",
    "step by step", "explain in detail", "debug", "refactor",
    "code review", "write a function", "write code", "write a program",
    "optimize", "algorithm", "strategy", "in depth", "comprehensive",
];

/// Heuristic query classifier with zero cost and zero latency.
pub struct QueryClassifier {
    /// Confidence threshold below which uncertain Simple classifications
    /// are upgraded to Standard (default UP rule).
    confidence_threshold: f32,
}

impl QueryClassifier {
    /// Create a new classifier with default confidence threshold.
    pub fn new() -> Self {
        Self {
            confidence_threshold: 0.4,
        }
    }

    /// Create a new classifier with a custom confidence threshold.
    pub fn with_threshold(confidence_threshold: f32) -> Self {
        Self {
            confidence_threshold,
        }
    }

    /// Classify a message's complexity using heuristic signals.
    ///
    /// Considers the current message text and recent conversation context
    /// (last 2-3 messages) to track conversation momentum.
    pub fn classify(&self, message: &str, recent_context: &[&str]) -> ClassificationResult {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return ClassificationResult {
                tier: ComplexityTier::Simple,
                confidence: 1.0,
                reason: "empty message",
            };
        }

        let mut score: i32 = 0;
        let lower = trimmed.to_lowercase();

        // Signal 1: Message length
        let word_count = trimmed.split_whitespace().count();
        score += Self::length_score(word_count);

        // Signal 2: Simple exact match
        if SIMPLE_EXACT.iter().any(|p| lower == *p) {
            score -= 3;
        }

        // Signal 3: Simple question patterns
        if SIMPLE_QUESTIONS.iter().any(|q| lower.contains(q)) {
            score -= 2;
        }

        // Signal 4: Complex indicators
        if COMPLEX_INDICATORS.iter().any(|c| lower.contains(c)) {
            score += 2;
        }

        // Signal 5: Code blocks
        if trimmed.contains("```") {
            score += 3;
        }

        // Signal 6: Multi-sentence detection
        let sentence_count = Self::count_sentences(trimmed);
        if sentence_count >= 3 {
            score += 1;
        }

        // Signal 7: Conversation momentum
        score += Self::momentum_score(recent_context);

        // Map score to tier
        let (tier, confidence, reason) = Self::score_to_tier(score);

        // Apply uncertainty default-up rule
        if confidence < self.confidence_threshold && tier == ComplexityTier::Simple {
            return ClassificationResult {
                tier: ComplexityTier::Standard,
                confidence,
                reason: "low confidence, defaulting up",
            };
        }

        ClassificationResult {
            tier,
            confidence,
            reason,
        }
    }

    fn length_score(word_count: usize) -> i32 {
        match word_count {
            0..=3 => -2,
            4..=15 => 0,
            16..=50 => 1,
            _ => 2,
        }
    }

    fn count_sentences(text: &str) -> usize {
        // Simple sentence counting: split on sentence-ending punctuation
        let mut count = 0;
        for c in text.chars() {
            if c == '.' || c == '?' || c == '!' {
                count += 1;
            }
        }
        // At least 1 sentence if there's text
        count.max(1)
    }

    fn momentum_score(recent_context: &[&str]) -> i32 {
        let limit = recent_context.len().min(3);
        let recent = &recent_context[recent_context.len().saturating_sub(limit)..];

        let complex_count = recent
            .iter()
            .filter(|m| {
                let lower = m.to_lowercase();
                COMPLEX_INDICATORS.iter().any(|c| lower.contains(c))
                    || m.contains("```")
            })
            .count();

        if complex_count >= 2 {
            1
        } else {
            0
        }
    }

    fn score_to_tier(score: i32) -> (ComplexityTier, f32, &'static str) {
        if score <= -2 {
            let confidence = ((-score) as f32 / 5.0).min(1.0);
            (ComplexityTier::Simple, confidence, "simple query indicators")
        } else if score >= 2 {
            let confidence = (score as f32 / 5.0).min(1.0);
            (ComplexityTier::Complex, confidence, "complex query indicators")
        } else {
            let confidence = 1.0 - (score.unsigned_abs() as f32 / 3.0);
            (ComplexityTier::Standard, confidence, "standard complexity")
        }
    }
}

impl Default for QueryClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_simple_greetings() {
        let c = QueryClassifier::new();
        assert_eq!(c.classify("hi", &[]).tier, ComplexityTier::Simple);
        assert_eq!(c.classify("hello", &[]).tier, ComplexityTier::Simple);
        assert_eq!(c.classify("thanks", &[]).tier, ComplexityTier::Simple);
        assert_eq!(c.classify("bye", &[]).tier, ComplexityTier::Simple);
        assert_eq!(c.classify("ok", &[]).tier, ComplexityTier::Simple);
    }

    #[test]
    fn classify_simple_questions() {
        let c = QueryClassifier::new();
        assert_eq!(
            c.classify("what time is it?", &[]).tier,
            ComplexityTier::Simple
        );
    }

    #[test]
    fn classify_complex_analysis() {
        let c = QueryClassifier::new();
        let result = c.classify(
            "tell me about the history of ancient Rome and compare their governance with modern democracy",
            &[],
        );
        assert_eq!(result.tier, ComplexityTier::Complex);
    }

    #[test]
    fn classify_complex_code() {
        let c = QueryClassifier::new();
        let result = c.classify(
            "analyze this code and refactor it for better performance",
            &[],
        );
        assert_eq!(result.tier, ComplexityTier::Complex);
    }

    #[test]
    fn classify_standard_moderate() {
        let c = QueryClassifier::new();
        // A moderate question that is neither simple nor complex
        let result = c.classify("what's the weather like today?", &[]);
        assert_eq!(result.tier, ComplexityTier::Standard);
    }

    #[test]
    fn classify_code_blocks_complex() {
        let c = QueryClassifier::new();
        let result = c.classify("can you fix this?\n```\nfn main() { panic!() }\n```", &[]);
        assert_eq!(result.tier, ComplexityTier::Complex);
    }

    #[test]
    fn uncertain_defaults_up() {
        // When confidence is low and tier is Simple, should default up to Standard
        let c = QueryClassifier::with_threshold(0.8); // High threshold to force default-up
        let result = c.classify("maybe", &[]);
        // "maybe" is short (score -2) but not in SIMPLE_EXACT, so score is just -2
        // confidence = 2/5 = 0.4, which is < 0.8 threshold
        assert_eq!(result.tier, ComplexityTier::Standard);
        assert_eq!(result.reason, "low confidence, defaulting up");
    }

    #[test]
    fn conversation_momentum_bias() {
        let c = QueryClassifier::new();
        let recent = &[
            "can you analyze the performance bottleneck?",
            "let me implement a better algorithm for this",
            "now debug the edge case",
        ];
        // Even a short message in a complex conversation should be biased
        let result = c.classify("what about this?", recent);
        // Momentum adds +1, so a neutral message leans complex-ish
        assert!(result.tier != ComplexityTier::Simple);
    }

    #[test]
    fn empty_message_is_simple() {
        let c = QueryClassifier::new();
        assert_eq!(c.classify("", &[]).tier, ComplexityTier::Simple);
        assert_eq!(c.classify("   ", &[]).tier, ComplexityTier::Simple);
    }

    #[test]
    fn tier_display() {
        assert_eq!(ComplexityTier::Simple.to_string(), "simple");
        assert_eq!(ComplexityTier::Standard.to_string(), "standard");
        assert_eq!(ComplexityTier::Complex.to_string(), "complex");
    }

    #[test]
    fn high_confidence_on_strong_signals() {
        let c = QueryClassifier::new();
        let result = c.classify("hi", &[]);
        assert!(result.confidence >= 0.8, "greetings should have high confidence");
    }
}
