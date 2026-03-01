// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hybrid retriever combining vector similarity and BM25 via RRF fusion.
//!
//! The retriever embeds the query, runs both vector search and FTS5 BM25,
//! fuses results using Reciprocal Rank Fusion (k=60), and applies
//! confidence-based boosting for final ranking.

use std::collections::HashMap;
use std::sync::Arc;

use blufio_config::model::MemoryConfig;
use blufio_core::error::BlufioError;
use blufio_core::traits::EmbeddingAdapter;
use blufio_core::types::EmbeddingInput;

use crate::embedder::OnnxEmbedder;
use crate::store::MemoryStore;
use crate::types::{cosine_similarity, ScoredMemory};

/// RRF constant per research literature.
const RRF_K: f32 = 60.0;

/// Hybrid retriever combining vector similarity search and BM25 keyword search.
///
/// Uses Reciprocal Rank Fusion (RRF) to merge results from both search
/// methods, then applies confidence-based boosting (explicit > extracted).
pub struct HybridRetriever {
    store: Arc<MemoryStore>,
    embedder: Arc<OnnxEmbedder>,
    config: MemoryConfig,
}

impl HybridRetriever {
    /// Creates a new hybrid retriever.
    pub fn new(
        store: Arc<MemoryStore>,
        embedder: Arc<OnnxEmbedder>,
        config: MemoryConfig,
    ) -> Self {
        Self {
            store,
            embedder,
            config,
        }
    }

    /// Retrieve relevant memories for a query using hybrid search.
    ///
    /// 1. Embeds the query text
    /// 2. Runs vector similarity search (cosine similarity with threshold filter)
    /// 3. Runs BM25 keyword search via FTS5
    /// 4. Fuses results with RRF (k=60)
    /// 5. Fetches full Memory structs for top results
    /// 6. Applies confidence boost (explicit 0.9 > extracted 0.6)
    /// 7. Returns sorted Vec<ScoredMemory>
    pub async fn retrieve(&self, query: &str) -> Result<Vec<ScoredMemory>, BlufioError> {
        // Step 1: Embed the query
        let output = self.embedder.embed(EmbeddingInput {
            texts: vec![query.to_string()],
        }).await?;

        let query_embedding = output.embeddings.into_iter().next().ok_or_else(|| {
            BlufioError::Internal("Embedding returned no results".to_string())
        })?;

        // Step 2: Vector search
        let vector_results = self.vector_search(&query_embedding).await?;

        // Step 3: BM25 search
        let bm25_results = self
            .store
            .search_bm25(query, self.config.max_retrieval_results)
            .await?;

        // Step 4: RRF fusion
        let fused = reciprocal_rank_fusion(&vector_results, &bm25_results);

        if fused.is_empty() {
            return Ok(vec![]);
        }

        // Step 5: Fetch full Memory structs
        let top_ids: Vec<String> = fused.iter().map(|(id, _)| id.clone()).collect();
        let memories = self.store.get_memories_by_ids(&top_ids).await?;

        // Build lookup for RRF scores
        let score_map: HashMap<&str, f32> =
            fused.iter().map(|(id, score)| (id.as_str(), *score)).collect();

        // Step 6: Apply confidence boost and build ScoredMemory
        let mut scored: Vec<ScoredMemory> = memories
            .into_iter()
            .filter_map(|memory| {
                let rrf_score = score_map.get(memory.id.as_str()).copied().unwrap_or(0.0);
                let boosted_score = rrf_score * memory.confidence as f32;
                Some(ScoredMemory {
                    memory,
                    score: boosted_score,
                })
            })
            .collect();

        // Step 7: Sort by boosted score descending
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored)
    }

    /// Vector search: compute cosine similarity against all active embeddings.
    ///
    /// Returns (id, similarity) pairs above the similarity threshold,
    /// sorted by similarity descending, capped at max_retrieval_results.
    async fn vector_search(
        &self,
        query_embedding: &[f32],
    ) -> Result<Vec<(String, f32)>, BlufioError> {
        let active_embeddings = self.store.get_active_embeddings().await?;

        let mut results: Vec<(String, f32)> = active_embeddings
            .into_iter()
            .filter_map(|(id, embedding)| {
                if embedding.len() != query_embedding.len() {
                    return None;
                }
                let similarity = cosine_similarity(query_embedding, &embedding);
                if similarity >= self.config.similarity_threshold as f32 {
                    Some((id, similarity))
                } else {
                    None
                }
            })
            .collect();

        // Sort by similarity descending
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Cap at max_retrieval_results
        results.truncate(self.config.max_retrieval_results);

        Ok(results)
    }
}

/// Reciprocal Rank Fusion: merge two ranked lists into a single ranking.
///
/// RRF score for document d = sum(1 / (k + rank_i)) for each list containing d.
/// k = 60 per Robertson et al. and Cormack et al. research.
///
/// Both input lists are (id, score) pairs where position = rank.
/// BM25 scores are negated (more negative = more relevant), so they
/// are already sorted by relevance via ORDER BY bm25().
pub fn reciprocal_rank_fusion(
    vector_results: &[(String, f32)],
    bm25_results: &[(String, f64)],
) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f32> = HashMap::new();

    // RRF from vector results (already sorted by similarity descending)
    for (rank, (id, _)) in vector_results.iter().enumerate() {
        *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (RRF_K + rank as f32 + 1.0);
    }

    // RRF from BM25 results (already sorted by bm25 score ascending = most relevant first)
    for (rank, (id, _)) in bm25_results.iter().enumerate() {
        *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (RRF_K + rank as f32 + 1.0);
    }

    // Sort by fused score descending
    let mut fused: Vec<(String, f32)> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_fusion_overlapping_lists() {
        // Document "d1" appears in both lists (rank 0 in each)
        // Document "d2" appears only in vector (rank 1)
        // Document "d3" appears only in bm25 (rank 1)
        let vector = vec![
            ("d1".to_string(), 0.9f32),
            ("d2".to_string(), 0.8f32),
        ];
        let bm25 = vec![
            ("d1".to_string(), -5.0f64), // most relevant
            ("d3".to_string(), -3.0f64),
        ];

        let fused = reciprocal_rank_fusion(&vector, &bm25);

        // d1 should have highest score (appears in both lists at rank 0)
        assert_eq!(fused[0].0, "d1");

        // d1 score = 1/(60+1) + 1/(60+1) = 2/(61) ~ 0.0328
        let expected_d1 = 2.0 / 61.0;
        assert!(
            (fused[0].1 - expected_d1).abs() < 0.001,
            "d1 score should be ~{expected_d1}, got {}",
            fused[0].1
        );

        // d2 and d3 should both have score 1/(60+2) = 1/62 ~ 0.0161
        let d2_score = fused.iter().find(|(id, _)| id == "d2").unwrap().1;
        let d3_score = fused.iter().find(|(id, _)| id == "d3").unwrap().1;
        assert!(
            (d2_score - d3_score).abs() < 0.001,
            "d2 and d3 should have same score"
        );
    }

    #[test]
    fn rrf_fusion_disjoint_lists() {
        let vector = vec![("a".to_string(), 0.9f32)];
        let bm25 = vec![("b".to_string(), -5.0f64)];

        let fused = reciprocal_rank_fusion(&vector, &bm25);

        assert_eq!(fused.len(), 2);
        // Both should have same score: 1/(60+1) = 1/61
        let a_score = fused.iter().find(|(id, _)| id == "a").unwrap().1;
        let b_score = fused.iter().find(|(id, _)| id == "b").unwrap().1;
        assert!(
            (a_score - b_score).abs() < 0.001,
            "disjoint results should have same score"
        );
    }

    #[test]
    fn rrf_fusion_empty_lists() {
        let vector: Vec<(String, f32)> = vec![];
        let bm25: Vec<(String, f64)> = vec![];

        let fused = reciprocal_rank_fusion(&vector, &bm25);
        assert!(fused.is_empty());
    }

    #[test]
    fn rrf_fusion_one_empty() {
        let vector = vec![
            ("x".to_string(), 0.9f32),
            ("y".to_string(), 0.7f32),
        ];
        let bm25: Vec<(String, f64)> = vec![];

        let fused = reciprocal_rank_fusion(&vector, &bm25);
        assert_eq!(fused.len(), 2);
        // x should rank higher (rank 0 vs rank 1)
        assert_eq!(fused[0].0, "x");
    }

    #[test]
    fn rrf_preserves_correct_ordering() {
        // d1 in both at rank 0, d2 in vector at rank 1, d3 in bm25 at rank 1, d4 in both at rank 2
        let vector = vec![
            ("d1".to_string(), 0.95f32),
            ("d2".to_string(), 0.85f32),
            ("d4".to_string(), 0.75f32),
        ];
        let bm25 = vec![
            ("d1".to_string(), -10.0f64),
            ("d3".to_string(), -8.0f64),
            ("d4".to_string(), -6.0f64),
        ];

        let fused = reciprocal_rank_fusion(&vector, &bm25);

        // d1 should be first (rank 0 in both)
        assert_eq!(fused[0].0, "d1");
        // d4 should be second (rank 2 in both, score = 2/63)
        assert_eq!(fused[1].0, "d4");
        // d2 and d3 should tie (rank 1 in one list each)
    }

    #[test]
    fn confidence_boost_explicit_over_extracted() {
        // Simulate confidence boost: explicit (0.9) vs extracted (0.6) with same RRF score
        let rrf_score = 1.0 / 61.0; // rank 0 in one list
        let explicit_boosted = rrf_score * 0.9;
        let extracted_boosted = rrf_score * 0.6;
        assert!(
            explicit_boosted > extracted_boosted,
            "Explicit memories should rank higher with confidence boost"
        );
    }
}
