// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hybrid retriever combining vector similarity and BM25 via RRF fusion.
//!
//! The retriever embeds the query, runs both vector search and FTS5 BM25,
//! fuses results using Reciprocal Rank Fusion (k=60), applies source-based
//! importance boost and temporal decay, then reranks with MMR for diversity.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tracing::warn;

use blufio_config::model::MemoryConfig;
use blufio_core::error::BlufioError;
use blufio_core::traits::EmbeddingAdapter;
use blufio_core::types::EmbeddingInput;

use crate::embedder::OnnxEmbedder;
use crate::store::MemoryStore;
use crate::types::{Memory, MemorySource, ScoredMemory, cosine_similarity};

/// RRF constant per research literature.
const RRF_K: f32 = 60.0;

/// Compute temporal decay factor for a memory based on its age.
///
/// File-sourced memories skip decay entirely (always 1.0).
/// Unparseable timestamps default to no decay (1.0) with a warning.
/// Formula: `max(decay_factor^days, decay_floor)`.
fn temporal_decay(memory: &Memory, now: chrono::DateTime<Utc>, config: &MemoryConfig) -> f32 {
    // FileWatcher memories skip temporal decay entirely
    if memory.source == MemorySource::FileWatcher {
        return 1.0;
    }

    let created = match chrono::DateTime::parse_from_rfc3339(&memory.created_at) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => {
            warn!(
                memory_id = %memory.id,
                created_at = %memory.created_at,
                "Unparseable created_at timestamp, skipping temporal decay"
            );
            return 1.0;
        }
    };

    let days = (now - created).num_days().max(0) as f32;
    (config.decay_factor as f32)
        .powf(days)
        .max(config.decay_floor as f32)
}

/// Return the importance boost multiplier for a given memory source.
fn importance_boost_for_source(source: &MemorySource, config: &MemoryConfig) -> f32 {
    match source {
        MemorySource::Explicit => config.importance_boost_explicit as f32,
        MemorySource::Extracted => config.importance_boost_extracted as f32,
        MemorySource::FileWatcher => config.importance_boost_file as f32,
    }
}

/// Hybrid retriever combining vector similarity search and BM25 keyword search.
///
/// Uses Reciprocal Rank Fusion (RRF) to merge results from both search
/// methods, applies importance boost and temporal decay, then reranks
/// with Maximal Marginal Relevance (MMR) for result diversity.
pub struct HybridRetriever {
    store: Arc<MemoryStore>,
    embedder: Arc<OnnxEmbedder>,
    config: MemoryConfig,
}

impl HybridRetriever {
    /// Creates a new hybrid retriever.
    pub fn new(store: Arc<MemoryStore>, embedder: Arc<OnnxEmbedder>, config: MemoryConfig) -> Self {
        Self {
            store,
            embedder,
            config,
        }
    }

    /// Retrieve relevant memories for a query using hybrid search.
    ///
    /// Pipeline:
    /// 1. Embed the query text
    /// 2. Run vector similarity search (cosine similarity with threshold filter)
    /// 3. Run BM25 keyword search via FTS5
    /// 4. Fuse results with RRF (k=60)
    /// 5. Fetch full Memory structs for top results
    /// 6. Apply importance boost and temporal decay: `rrf_score * importance * decay`
    /// 7. Sort by combined score descending
    /// 8. MMR diversity reranking (greedy, lambda-weighted)
    /// 9. Return Vec<ScoredMemory>
    pub async fn retrieve(&self, query: &str) -> Result<Vec<ScoredMemory>, BlufioError> {
        // Step 1: Embed the query
        let output = self
            .embedder
            .embed(EmbeddingInput {
                texts: vec![query.to_string()],
            })
            .await?;

        let query_embedding =
            output.embeddings.into_iter().next().ok_or_else(|| {
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
        let score_map: HashMap<&str, f32> = fused
            .iter()
            .map(|(id, score)| (id.as_str(), *score))
            .collect();

        // Step 6: Apply importance boost + temporal decay
        let now = Utc::now();
        let mut scored: Vec<ScoredMemory> = memories
            .into_iter()
            .map(|memory| {
                let rrf_score = score_map.get(memory.id.as_str()).copied().unwrap_or(0.0);
                let importance = importance_boost_for_source(&memory.source, &self.config);
                let decay = temporal_decay(&memory, now, &self.config);
                let final_score = rrf_score * importance * decay;
                ScoredMemory {
                    memory,
                    score: final_score,
                }
            })
            .collect();

        // Step 7: Sort by combined score descending
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Step 8: MMR diversity reranking
        let result = mmr_rerank(
            &scored,
            self.config.mmr_lambda,
            self.config.max_retrieval_results,
        );

        Ok(result)
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

/// Maximal Marginal Relevance (MMR) diversity reranking.
///
/// Greedy selection per Carbonell & Goldstein (1998):
/// - First pick: always the highest-scored item.
/// - Each subsequent pick maximizes:
///   `lambda * relevance - (1 - lambda) * max_similarity_to_already_selected`
///
/// `lambda = 1.0` preserves pure relevance ordering (no diversity penalty).
/// `lambda = 0.0` maximizes diversity (most dissimilar selections).
///
/// The input `scored` slice must already be sorted by score descending.
/// Returns up to `k` items.
fn mmr_rerank(scored: &[ScoredMemory], lambda: f64, k: usize) -> Vec<ScoredMemory> {
    if scored.is_empty() || k == 0 {
        return vec![];
    }

    let n = scored.len();
    let take = k.min(n);
    let lambda_f = lambda as f32;

    // Normalize relevance scores to [0, 1] for MMR formula balance.
    // The highest score becomes 1.0 and lowest becomes 0.0 (or all 1.0 if identical).
    let max_score = scored[0].score;
    let min_score = scored[n - 1].score;
    let score_range = max_score - min_score;

    let norm_relevance: Vec<f32> = scored
        .iter()
        .map(|s| {
            if score_range.abs() < f32::EPSILON {
                1.0
            } else {
                (s.score - min_score) / score_range
            }
        })
        .collect();

    let mut selected_indices: Vec<usize> = Vec::with_capacity(take);
    let mut remaining: Vec<usize> = (0..n).collect();

    // First pick: highest score (index 0 since input is sorted descending)
    selected_indices.push(0);
    remaining.retain(|&i| i != 0);

    // Greedy selection for remaining picks
    while selected_indices.len() < take && !remaining.is_empty() {
        let mut best_idx = remaining[0];
        let mut best_mmr = f32::NEG_INFINITY;

        for &candidate in &remaining {
            let relevance = norm_relevance[candidate];

            // Max similarity to any already-selected item
            let max_sim = selected_indices
                .iter()
                .map(|&sel| {
                    let cand_emb = &scored[candidate].memory.embedding;
                    let sel_emb = &scored[sel].memory.embedding;
                    if cand_emb.is_empty() || sel_emb.is_empty() || cand_emb.len() != sel_emb.len()
                    {
                        0.0
                    } else {
                        cosine_similarity(cand_emb, sel_emb)
                    }
                })
                .fold(f32::NEG_INFINITY, f32::max);

            let mmr_score = lambda_f * relevance - (1.0 - lambda_f) * max_sim;

            if mmr_score > best_mmr {
                best_mmr = mmr_score;
                best_idx = candidate;
            }
        }

        selected_indices.push(best_idx);
        remaining.retain(|&i| i != best_idx);
    }

    selected_indices
        .into_iter()
        .map(|i| scored[i].clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_fusion_overlapping_lists() {
        // Document "d1" appears in both lists (rank 0 in each)
        // Document "d2" appears only in vector (rank 1)
        // Document "d3" appears only in bm25 (rank 1)
        let vector = vec![("d1".to_string(), 0.9f32), ("d2".to_string(), 0.8f32)];
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
        let vector = vec![("x".to_string(), 0.9f32), ("y".to_string(), 0.7f32)];
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

    // --- Helper to create test memories ---

    fn make_memory(id: &str, source: MemorySource, created_at: &str) -> Memory {
        use crate::types::MemoryStatus;
        use blufio_core::classification::DataClassification;
        Memory {
            id: id.to_string(),
            content: format!("memory {id}"),
            embedding: vec![],
            source,
            confidence: 1.0,
            status: MemoryStatus::Active,
            superseded_by: None,
            session_id: None,
            classification: DataClassification::default(),
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
        }
    }

    fn default_config() -> MemoryConfig {
        MemoryConfig::default()
    }

    // --- temporal_decay tests ---

    #[test]
    fn temporal_decay_today_returns_one() {
        let config = default_config();
        let now = Utc::now();
        let mem = make_memory("m1", MemorySource::Explicit, &now.to_rfc3339());
        let decay = temporal_decay(&mem, now, &config);
        assert!(
            (decay - 1.0).abs() < 0.001,
            "Memory created now should have decay ~1.0, got {decay}"
        );
    }

    #[test]
    fn temporal_decay_seven_days_old() {
        let config = default_config();
        let now = Utc::now();
        let seven_days_ago = now - chrono::Duration::days(7);
        let mem = make_memory("m2", MemorySource::Explicit, &seven_days_ago.to_rfc3339());
        let decay = temporal_decay(&mem, now, &config);
        let expected = 0.95_f32.powf(7.0);
        assert!(
            (decay - expected).abs() < 0.001,
            "7-day-old memory should have decay ~{expected}, got {decay}"
        );
    }

    #[test]
    fn temporal_decay_very_old_hits_floor() {
        let config = default_config();
        let now = Utc::now();
        let old = now - chrono::Duration::days(1000);
        let mem = make_memory("m3", MemorySource::Explicit, &old.to_rfc3339());
        let decay = temporal_decay(&mem, now, &config);
        assert!(
            (decay - config.decay_floor as f32).abs() < 0.001,
            "Very old memory should hit floor {}, got {decay}",
            config.decay_floor
        );
    }

    #[test]
    fn temporal_decay_file_watcher_always_one() {
        let config = default_config();
        let now = Utc::now();
        let old = now - chrono::Duration::days(365);
        let mem = make_memory("m4", MemorySource::FileWatcher, &old.to_rfc3339());
        let decay = temporal_decay(&mem, now, &config);
        assert!(
            (decay - 1.0).abs() < f32::EPSILON,
            "FileWatcher memory should always have decay 1.0, got {decay}"
        );
    }

    #[test]
    fn temporal_decay_unparseable_timestamp_returns_one() {
        let config = default_config();
        let now = Utc::now();
        let mem = make_memory("m5", MemorySource::Extracted, "not-a-date");
        let decay = temporal_decay(&mem, now, &config);
        assert!(
            (decay - 1.0).abs() < f32::EPSILON,
            "Unparseable timestamp should return 1.0, got {decay}"
        );
    }

    // --- importance_boost_for_source tests ---

    #[test]
    fn importance_boost_explicit() {
        let config = default_config();
        let boost = importance_boost_for_source(&MemorySource::Explicit, &config);
        assert!(
            (boost - 1.0).abs() < f32::EPSILON,
            "Explicit boost should be 1.0, got {boost}"
        );
    }

    #[test]
    fn importance_boost_extracted() {
        let config = default_config();
        let boost = importance_boost_for_source(&MemorySource::Extracted, &config);
        assert!(
            (boost - 0.6).abs() < 0.001,
            "Extracted boost should be 0.6, got {boost}"
        );
    }

    #[test]
    fn importance_boost_file_watcher() {
        let config = default_config();
        let boost = importance_boost_for_source(&MemorySource::FileWatcher, &config);
        assert!(
            (boost - 0.8).abs() < 0.001,
            "FileWatcher boost should be 0.8, got {boost}"
        );
    }

    // --- Combined scoring formula tests ---

    #[test]
    fn scoring_formula_is_multiplicative() {
        let config = default_config();
        let now = Utc::now();
        let mem = make_memory("m6", MemorySource::Explicit, &now.to_rfc3339());
        let rrf_score = 0.5_f32;
        let importance = importance_boost_for_source(&mem.source, &config);
        let decay = temporal_decay(&mem, now, &config);
        let final_score = rrf_score * importance * decay;
        // Explicit today: 0.5 * 1.0 * 1.0 = 0.5
        assert!(
            (final_score - 0.5).abs() < 0.001,
            "Score should be rrf * importance * decay = 0.5, got {final_score}"
        );
    }

    #[test]
    fn explicit_ranks_above_extracted_same_rrf() {
        let config = default_config();
        let now = Utc::now();
        let explicit = make_memory("e1", MemorySource::Explicit, &now.to_rfc3339());
        let extracted = make_memory("e2", MemorySource::Extracted, &now.to_rfc3339());
        let rrf = 0.5_f32;

        let score_explicit = rrf
            * importance_boost_for_source(&explicit.source, &config)
            * temporal_decay(&explicit, now, &config);
        let score_extracted = rrf
            * importance_boost_for_source(&extracted.source, &config)
            * temporal_decay(&extracted, now, &config);

        assert!(
            score_explicit > score_extracted,
            "Explicit ({score_explicit}) should rank above Extracted ({score_extracted})"
        );
    }

    #[test]
    fn older_memory_ranks_below_newer_same_source_and_rrf() {
        let config = default_config();
        let now = Utc::now();
        let newer = make_memory("n1", MemorySource::Explicit, &now.to_rfc3339());
        let older_time = now - chrono::Duration::days(30);
        let older = make_memory("o1", MemorySource::Explicit, &older_time.to_rfc3339());
        let rrf = 0.5_f32;

        let score_newer = rrf
            * importance_boost_for_source(&newer.source, &config)
            * temporal_decay(&newer, now, &config);
        let score_older = rrf
            * importance_boost_for_source(&older.source, &config)
            * temporal_decay(&older, now, &config);

        assert!(
            score_newer > score_older,
            "Newer ({score_newer}) should rank above older ({score_older})"
        );
    }

    // --- MMR helper to create scored memories with embeddings ---

    fn make_scored(id: &str, score: f32, embedding: Vec<f32>) -> ScoredMemory {
        let mut mem = make_memory(id, MemorySource::Explicit, "2026-03-01T00:00:00Z");
        mem.embedding = embedding;
        ScoredMemory { memory: mem, score }
    }

    // --- mmr_rerank tests ---

    #[test]
    fn mmr_rerank_empty_input() {
        let result = mmr_rerank(&[], 0.7, 5);
        assert!(result.is_empty());
    }

    #[test]
    fn mmr_rerank_k_zero_returns_empty() {
        let scored = vec![make_scored("a", 1.0, vec![1.0, 0.0, 0.0])];
        let result = mmr_rerank(&scored, 0.7, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn mmr_rerank_lambda_one_preserves_relevance_order() {
        // lambda=1.0 means no diversity penalty -- pure relevance
        let scored = vec![
            make_scored("a", 0.9, vec![1.0, 0.0, 0.0]),
            make_scored("b", 0.7, vec![0.98, 0.2, 0.0]),
            make_scored("c", 0.5, vec![0.0, 1.0, 0.0]),
        ];
        let result = mmr_rerank(&scored, 1.0, 3);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].memory.id, "a");
        assert_eq!(result[1].memory.id, "b");
        assert_eq!(result[2].memory.id, "c");
    }

    #[test]
    fn mmr_rerank_lambda_zero_maximizes_diversity() {
        // lambda=0.0 means pure diversity (select most dissimilar to selected set)
        // a and b are very similar, c is orthogonal
        let scored = vec![
            make_scored("a", 0.9, vec![1.0, 0.0, 0.0]),
            make_scored("b", 0.8, vec![0.98, 0.2, 0.0]), // very similar to a
            make_scored("c", 0.7, vec![0.0, 1.0, 0.0]),  // orthogonal to a
        ];
        let result = mmr_rerank(&scored, 0.0, 3);
        assert_eq!(result.len(), 3);
        // First is always highest score
        assert_eq!(result[0].memory.id, "a");
        // Second should be c (orthogonal = most dissimilar from a)
        assert_eq!(
            result[1].memory.id, "c",
            "With lambda=0, most dissimilar (c) should be picked before similar (b)"
        );
        // Third is b (remaining)
        assert_eq!(result[2].memory.id, "b");
    }

    #[test]
    fn mmr_rerank_diversity_promotes_dissimilar() {
        // 3 similar memories + 1 dissimilar, default lambda=0.7
        // MMR should promote the dissimilar one earlier than pure relevance
        let scored = vec![
            make_scored("s1", 0.9, vec![1.0, 0.0, 0.0]),
            make_scored("s2", 0.85, vec![0.98, 0.2, 0.0]), // similar to s1
            make_scored("s3", 0.8, vec![0.95, 0.31, 0.0]), // similar to s1
            make_scored("d1", 0.75, vec![0.0, 1.0, 0.0]),  // dissimilar (orthogonal)
        ];
        let result = mmr_rerank(&scored, 0.7, 4);
        assert_eq!(result.len(), 4);
        // s1 always first (highest score)
        assert_eq!(result[0].memory.id, "s1");
        // d1 should appear before s3 (diversity boost overcomes slight score gap)
        let d1_pos = result.iter().position(|r| r.memory.id == "d1").unwrap();
        let s3_pos = result.iter().position(|r| r.memory.id == "s3").unwrap();
        assert!(
            d1_pos < s3_pos,
            "Dissimilar d1 (pos {d1_pos}) should be promoted before similar s3 (pos {s3_pos})"
        );
    }

    #[test]
    fn mmr_rerank_k_larger_than_input() {
        let scored = vec![
            make_scored("a", 0.9, vec![1.0, 0.0, 0.0]),
            make_scored("b", 0.7, vec![0.0, 1.0, 0.0]),
        ];
        let result = mmr_rerank(&scored, 0.7, 10);
        assert_eq!(
            result.len(),
            2,
            "Should return all items when k > input len"
        );
    }

    #[test]
    fn mmr_rerank_first_item_is_highest_scored() {
        let scored = vec![
            make_scored("top", 1.0, vec![1.0, 0.0, 0.0]),
            make_scored("mid", 0.5, vec![0.0, 1.0, 0.0]),
            make_scored("low", 0.1, vec![0.0, 0.0, 1.0]),
        ];
        for lambda in [0.0, 0.3, 0.5, 0.7, 1.0] {
            let result = mmr_rerank(&scored, lambda, 3);
            assert_eq!(
                result[0].memory.id, "top",
                "First item must always be highest scored (lambda={lambda})"
            );
        }
    }

    #[test]
    fn mmr_rerank_single_item() {
        let scored = vec![make_scored("only", 0.5, vec![1.0, 0.0, 0.0])];
        let result = mmr_rerank(&scored, 0.7, 5);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].memory.id, "only");
    }
}
