// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Memory domain types for the long-term memory system.

use serde::{Deserialize, Serialize};

/// A single memory fact stored by the memory system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Unique identifier for this memory.
    pub id: String,
    /// The factual content of this memory.
    pub content: String,
    /// Embedding vector for semantic search (384-dim for all-MiniLM-L6-v2).
    #[serde(skip)]
    pub embedding: Vec<f32>,
    /// How this memory was created.
    pub source: MemorySource,
    /// Confidence score (0.0-1.0). Explicit > Extracted.
    pub confidence: f64,
    /// Current lifecycle status.
    pub status: MemoryStatus,
    /// If superseded, the ID of the newer memory.
    pub superseded_by: Option<String>,
    /// Session where this memory was created.
    pub session_id: Option<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 last-update timestamp.
    pub updated_at: String,
}

/// How a memory was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemorySource {
    /// User explicitly said "remember this".
    Explicit,
    /// LLM extracted from conversation at end of session.
    Extracted,
}

impl MemorySource {
    /// Convert to string for SQLite storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            MemorySource::Explicit => "explicit",
            MemorySource::Extracted => "extracted",
        }
    }

    /// Parse from SQLite string.
    pub fn from_str_value(s: &str) -> Self {
        match s {
            "explicit" => MemorySource::Explicit,
            _ => MemorySource::Extracted,
        }
    }
}

/// Lifecycle status of a memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryStatus {
    /// Active and available for retrieval.
    Active,
    /// Replaced by a newer, contradicting memory.
    Superseded,
    /// User explicitly asked to forget this.
    Forgotten,
}

impl MemoryStatus {
    /// Convert to string for SQLite storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryStatus::Active => "active",
            MemoryStatus::Superseded => "superseded",
            MemoryStatus::Forgotten => "forgotten",
        }
    }

    /// Parse from SQLite string.
    pub fn from_str_value(s: &str) -> Self {
        match s {
            "superseded" => MemoryStatus::Superseded,
            "forgotten" => MemoryStatus::Forgotten,
            _ => MemoryStatus::Active,
        }
    }
}

/// A memory with a retrieval score from hybrid search.
#[derive(Debug, Clone)]
pub struct ScoredMemory {
    /// The memory fact.
    pub memory: Memory,
    /// Combined retrieval score (RRF + confidence boost).
    pub score: f32,
}

/// A fact extracted from conversation by the LLM.
#[derive(Debug, Clone, Deserialize)]
pub struct ExtractedFact {
    /// The fact content as a standalone statement.
    pub content: String,
    /// Category: personal, preference, project, decision, instruction, outcome.
    pub category: String,
}

/// Result of a memory extraction operation.
#[derive(Debug)]
pub struct ExtractionResult {
    /// Newly created memories.
    pub memories: Vec<Memory>,
    /// Token usage from the extraction LLM call (for cost tracking).
    pub usage: Option<blufio_core::types::TokenUsage>,
}

/// Convert f32 vector to bytes for SQLite BLOB storage.
pub fn vec_to_blob(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Convert SQLite BLOB back to f32 vector.
pub fn blob_to_vec(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

/// Compute cosine similarity between two vectors.
///
/// For L2-normalized vectors (as output by sentence transformers),
/// this is equivalent to the dot product.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "vectors must have same length");
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_input_output_types() {
        let input = blufio_core::types::EmbeddingInput {
            texts: vec!["hello world".to_string()],
        };
        assert_eq!(input.texts.len(), 1);

        let output = blufio_core::types::EmbeddingOutput {
            embeddings: vec![vec![0.1, 0.2, 0.3]],
            dimensions: 3,
        };
        assert_eq!(output.embeddings.len(), 1);
        assert_eq!(output.dimensions, 3);
    }

    #[test]
    fn memory_struct_fields() {
        let memory = Memory {
            id: "test-id".to_string(),
            content: "User's dog is named Max".to_string(),
            embedding: vec![0.1; 384],
            source: MemorySource::Explicit,
            confidence: 0.9,
            status: MemoryStatus::Active,
            superseded_by: None,
            session_id: Some("session-1".to_string()),
            created_at: "2026-03-01T00:00:00Z".to_string(),
            updated_at: "2026-03-01T00:00:00Z".to_string(),
        };
        assert_eq!(memory.id, "test-id");
        assert_eq!(memory.embedding.len(), 384);
    }

    #[test]
    fn memory_source_variants() {
        assert_eq!(MemorySource::Explicit.as_str(), "explicit");
        assert_eq!(MemorySource::Extracted.as_str(), "extracted");
        assert_eq!(MemorySource::from_str_value("explicit"), MemorySource::Explicit);
        assert_eq!(MemorySource::from_str_value("extracted"), MemorySource::Extracted);
    }

    #[test]
    fn memory_status_variants() {
        assert_eq!(MemoryStatus::Active.as_str(), "active");
        assert_eq!(MemoryStatus::Superseded.as_str(), "superseded");
        assert_eq!(MemoryStatus::Forgotten.as_str(), "forgotten");
        assert_eq!(MemoryStatus::from_str_value("active"), MemoryStatus::Active);
        assert_eq!(MemoryStatus::from_str_value("superseded"), MemoryStatus::Superseded);
        assert_eq!(MemoryStatus::from_str_value("forgotten"), MemoryStatus::Forgotten);
    }

    #[test]
    fn vec_to_blob_roundtrip() {
        let original = vec![0.1_f32, 0.2, 0.3, -0.5, 1.0];
        let blob = vec_to_blob(&original);
        let recovered = blob_to_vec(&blob);
        assert_eq!(original.len(), recovered.len());
        for (a, b) in original.iter().zip(recovered.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn vec_to_blob_384_dim() {
        let vec384: Vec<f32> = (0..384).map(|i| i as f32 / 384.0).collect();
        let blob = vec_to_blob(&vec384);
        assert_eq!(blob.len(), 384 * 4); // 1536 bytes
        let recovered = blob_to_vec(&blob);
        assert_eq!(recovered.len(), 384);
    }

    #[test]
    fn cosine_similarity_identical_normalized() {
        // Normalized vector
        let v: Vec<f32> = vec![0.5773, 0.5773, 0.5773]; // ~1/sqrt(3) each
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 0.01, "identical normalized vectors should have sim ~1.0, got {sim}");
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < f32::EPSILON, "orthogonal vectors should have sim ~0.0, got {sim}");
    }

    #[test]
    fn cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < f32::EPSILON, "opposite vectors should have sim ~-1.0, got {sim}");
    }
}
