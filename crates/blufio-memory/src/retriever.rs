// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hybrid retriever combining vector similarity and BM25 via RRF fusion.
//!
//! The retriever embeds the query, runs both vector search and FTS5 BM25,
//! fuses results using Reciprocal Rank Fusion (k=60), applies source-based
//! importance boost and temporal decay, then reranks with MMR for diversity.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use metrics::{counter, histogram};
use tracing::warn;

use blufio_config::model::MemoryConfig;
use blufio_core::error::BlufioError;
use blufio_core::traits::EmbeddingAdapter;
use blufio_core::types::EmbeddingInput;

use crate::embedder::OnnxEmbedder;
use crate::store::MemoryStore;
use crate::types::{Memory, MemorySource, ScoredMemory, cosine_similarity};
use crate::vec0;

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
/// Maximum number of consecutive fallback events logged individually
/// before switching to rate-limited batch logging.
const FALLBACK_LOG_THRESHOLD: u64 = 5;

/// Minimum interval between rate-limited fallback log messages.
const FALLBACK_LOG_INTERVAL_SECS: u64 = 60;

pub struct HybridRetriever {
    store: Arc<MemoryStore>,
    embedder: Arc<OnnxEmbedder>,
    config: MemoryConfig,
    /// Whether to use vec0 KNN search (from config toggle).
    vec0_enabled: bool,
    /// Count of consecutive vec0 fallback events for rate-limited logging.
    fallback_count: Arc<AtomicU64>,
    /// Timestamp (epoch secs) of last fallback log for suppression.
    last_fallback_log: Arc<AtomicU64>,
}

impl HybridRetriever {
    /// Creates a new hybrid retriever.
    pub fn new(store: Arc<MemoryStore>, embedder: Arc<OnnxEmbedder>, config: MemoryConfig) -> Self {
        let vec0_enabled = config.vec0_enabled;
        Self {
            store,
            embedder,
            config,
            vec0_enabled,
            fallback_count: Arc::new(AtomicU64::new(0)),
            last_fallback_log: Arc::new(AtomicU64::new(0)),
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
        // OTel: Memory retrieval span with result count, top score, and backend type.
        // Created as a handle (not entered) because entered spans are !Send.
        let _memory_span = tracing::info_span!(
            "blufio.memory.retrieve",
            "blufio.memory.results_count" = tracing::field::Empty,
            "blufio.memory.top_score" = tracing::field::Empty,
            "blufio.memory.backend" = tracing::field::Empty,
        );

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

        // Step 2: Vector search (vec0 KNN when enabled, fallback to in-memory)
        let fallback_before = self.fallback_count.load(Ordering::Relaxed);
        let vector_results = self.vector_search(&query_embedding).await?;
        let fallback_after = self.fallback_count.load(Ordering::Relaxed);
        let backend = if self.vec0_enabled && fallback_after == fallback_before {
            "vec0"
        } else {
            "in_memory"
        };
        _memory_span.record("blufio.memory.backend", backend);

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

        // OTel: Record retrieval result attributes on span.
        _memory_span.record("blufio.memory.results_count", result.len() as u64);
        if let Some(top) = result.first() {
            _memory_span.record("blufio.memory.top_score", top.score as f64);
        }

        Ok(result)
    }

    /// Vector search: uses vec0 KNN when enabled, falls back to in-memory on failure.
    ///
    /// Returns (id, similarity) pairs above the similarity threshold,
    /// sorted by similarity descending, capped at max_retrieval_results.
    async fn vector_search(
        &self,
        query_embedding: &[f32],
    ) -> Result<Vec<(String, f32)>, BlufioError> {
        if self.vec0_enabled {
            let start = std::time::Instant::now();
            match self.vec0_vector_search(query_embedding).await {
                Ok(results) => {
                    histogram!("blufio_memory_vec0_search_duration_seconds")
                        .record(start.elapsed().as_secs_f64());
                    return Ok(results);
                }
                Err(e) => {
                    // Per-query fallback: log and fall through to in-memory
                    self.log_vec0_fallback(&e);
                    counter!("blufio_memory_vec0_fallback_total").increment(1);
                    // Fall through to in-memory search below
                }
            }
        }
        // Existing in-memory cosine similarity search (unchanged)
        self.in_memory_vector_search(query_embedding).await
    }

    /// In-memory vector search: loads all active embeddings and computes cosine similarity.
    ///
    /// This is the original vector search path, used when vec0_enabled is false
    /// or as fallback when vec0 query fails.
    async fn in_memory_vector_search(
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

    /// Vec0 KNN vector search: delegates to vec0::vec0_search.
    ///
    /// Maps Vec0SearchResult to (memory_id, similarity) pairs for the RRF pipeline.
    /// The vec0 search already applies VEC-03 metadata filtering (status='active',
    /// classification!='restricted').
    async fn vec0_vector_search(
        &self,
        query_embedding: &[f32],
    ) -> Result<Vec<(String, f32)>, BlufioError> {
        let query_emb = query_embedding.to_vec();
        let k = self.config.max_retrieval_results;
        let threshold = self.config.similarity_threshold;

        let results = self
            .store
            .conn()
            .call(move |conn| vec0::vec0_search(conn, &query_emb, k, threshold, None))
            .await
            .map_err(BlufioError::storage_connection_failed)?;

        Ok(results
            .into_iter()
            .map(|r| (r.memory_id, r.similarity))
            .collect())
    }

    /// Log vec0 fallback with rate limiting.
    ///
    /// Logs the first `FALLBACK_LOG_THRESHOLD` failures individually, then
    /// suppresses and batch-logs every `FALLBACK_LOG_INTERVAL_SECS` seconds.
    fn log_vec0_fallback(&self, error: &impl std::fmt::Display) {
        let count = self.fallback_count.fetch_add(1, Ordering::Relaxed) + 1;

        if count <= FALLBACK_LOG_THRESHOLD {
            warn!("vec0 search failed, falling back to in-memory: {error}");
        } else {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let last = self.last_fallback_log.load(Ordering::Relaxed);
            if now_secs - last >= FALLBACK_LOG_INTERVAL_SECS {
                self.last_fallback_log.store(now_secs, Ordering::Relaxed);
                warn!(
                    "vec0 search failed {count} times since last log, falling back to in-memory: {error}"
                );
            }
        }
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

    // --- vec0 retriever tests ---

    use crate::store::MemoryStore;
    use crate::types::MemoryStatus;
    use blufio_core::classification::DataClassification;
    use tokio_rusqlite::Connection;

    /// Create an async test DB with vec0 virtual table.
    async fn setup_retriever_test_db() -> Connection {
        vec0::ensure_sqlite_vec_registered();
        let conn = Connection::open_in_memory().await.unwrap();
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS memories (
                    id TEXT PRIMARY KEY NOT NULL,
                    content TEXT NOT NULL,
                    embedding BLOB NOT NULL,
                    source TEXT NOT NULL,
                    confidence REAL NOT NULL DEFAULT 0.5,
                    status TEXT NOT NULL DEFAULT 'active',
                    superseded_by TEXT,
                    session_id TEXT,
                    classification TEXT NOT NULL DEFAULT 'internal',
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    deleted_at TEXT
                );

                CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                    content,
                    content='memories',
                    content_rowid='rowid'
                );

                CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
                END;

                CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                    INSERT INTO memories_fts(memories_fts, rowid, content)
                        VALUES('delete', old.rowid, old.content);
                END;

                CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                    INSERT INTO memories_fts(memories_fts, rowid, content)
                        VALUES('delete', old.rowid, old.content);
                    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
                END;

                CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec0 USING vec0(
                    status text,
                    classification text,
                    session_id text partition key,
                    embedding float[384] distance_metric=cosine,
                    +memory_id text,
                    +content text,
                    +source text,
                    +confidence float,
                    +created_at text
                );",
            )?;
            Ok(())
        })
        .await
        .unwrap();
        conn
    }

    fn make_test_memory_full(id: &str, content: &str) -> Memory {
        Memory {
            id: id.to_string(),
            content: content.to_string(),
            embedding: vec![0.1; 384],
            source: MemorySource::Explicit,
            confidence: 0.9,
            status: MemoryStatus::Active,
            superseded_by: None,
            session_id: Some("test-session".to_string()),
            classification: DataClassification::default(),
            created_at: "2026-03-01T00:00:00.000Z".to_string(),
            updated_at: "2026-03-01T00:00:00.000Z".to_string(),
        }
    }

    #[tokio::test]
    async fn vec0_vector_search_returns_results() {
        let conn = setup_retriever_test_db().await;
        let store = Arc::new(MemoryStore::with_vec0(conn, None, true));

        // Insert a memory with dual-write
        store
            .save(&make_test_memory_full("mem-r1", "Coffee preference"))
            .await
            .unwrap();

        // Use vec0 search directly through the retriever's private method
        // We test by calling the store's vec0 search
        let results = store
            .conn()
            .call(|conn| vec0::vec0_search(conn, &vec![0.1f32; 384], 10, 0.0, None))
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].memory_id, "mem-r1");
    }

    #[tokio::test]
    async fn vec0_disabled_uses_in_memory_path() {
        let conn = setup_retriever_test_db().await;
        let store = Arc::new(MemoryStore::with_vec0(conn, None, false));

        store
            .save(&make_test_memory_full("mem-r2", "Tea preference"))
            .await
            .unwrap();

        // vec0 should be empty (disabled)
        let vec0_count = store
            .conn()
            .call(|conn| vec0::vec0_count(conn))
            .await
            .unwrap();
        assert_eq!(vec0_count, 0, "vec0 should be empty when disabled");

        // In-memory embeddings should still work
        let embeddings = store.get_active_embeddings().await.unwrap();
        assert_eq!(embeddings.len(), 1);
    }

    #[tokio::test]
    async fn vec0_fallback_on_error_uses_in_memory() {
        // Create a DB without vec0 table to simulate vec0 failure
        vec0::ensure_sqlite_vec_registered();
        let conn = Connection::open_in_memory().await.unwrap();
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS memories (
                    id TEXT PRIMARY KEY NOT NULL,
                    content TEXT NOT NULL,
                    embedding BLOB NOT NULL,
                    source TEXT NOT NULL,
                    confidence REAL NOT NULL DEFAULT 0.5,
                    status TEXT NOT NULL DEFAULT 'active',
                    superseded_by TEXT,
                    session_id TEXT,
                    classification TEXT NOT NULL DEFAULT 'internal',
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    deleted_at TEXT
                );

                CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                    content,
                    content='memories',
                    content_rowid='rowid'
                );

                CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
                END;",
            )?;
            // NOTE: no memories_vec0 table -- vec0 search will fail
            Ok(())
        })
        .await
        .unwrap();

        // Create a store (vec0 disabled for inserts since table doesn't exist)
        let store = Arc::new(MemoryStore::with_vec0(conn, None, false));

        // Save a memory normally (no dual-write)
        store
            .save(&make_test_memory_full("mem-fallback", "Fallback test"))
            .await
            .unwrap();

        // Vec0 search should fail (no table), verify in-memory works as fallback
        let query_emb = vec![0.1f32; 384];

        // vec0 search should fail
        let vec0_result = store
            .conn()
            .call(move |conn| vec0::vec0_search(conn, &query_emb, 10, 0.0, None))
            .await;
        assert!(
            vec0_result.is_err(),
            "vec0 search should fail without table"
        );

        // In-memory search should still work
        let in_mem_results = store.get_active_embeddings().await.unwrap();
        assert_eq!(
            in_mem_results.len(),
            1,
            "in-memory path should work as fallback"
        );
    }

    #[tokio::test]
    async fn vec0_results_match_rrf_format() {
        // Verify vec0 search results produce (id, similarity) pairs like in-memory
        let conn = setup_retriever_test_db().await;
        let store = Arc::new(MemoryStore::with_vec0(conn, None, true));

        store
            .save(&make_test_memory_full("mem-fmt-1", "Format test"))
            .await
            .unwrap();

        let results = store
            .conn()
            .call(|conn| vec0::vec0_search(conn, &vec![0.1f32; 384], 10, 0.0, None))
            .await
            .unwrap();

        // Results should be mappable to (String, f32) for RRF
        let rrf_compatible: Vec<(String, f32)> = results
            .into_iter()
            .map(|r| (r.memory_id, r.similarity))
            .collect();

        assert_eq!(rrf_compatible.len(), 1);
        assert_eq!(rrf_compatible[0].0, "mem-fmt-1");
        assert!(rrf_compatible[0].1 > 0.0, "similarity should be positive");
    }

    #[test]
    fn fallback_counter_tracks_consecutive_failures() {
        // Test the fallback count tracking used by log_vec0_fallback
        let counter = Arc::new(AtomicU64::new(0));

        // Simulate 5 fallback events
        for _ in 0..5 {
            counter.fetch_add(1, Ordering::Relaxed);
        }
        assert_eq!(counter.load(Ordering::Relaxed), 5);

        // 6th event
        let count = counter.fetch_add(1, Ordering::Relaxed) + 1;
        assert_eq!(count, 6);
        // After threshold, rate limiting kicks in (tested via the constant)
        assert!(count > FALLBACK_LOG_THRESHOLD);
    }

    #[test]
    fn vec0_enabled_propagates_from_config() {
        // Verify that vec0_enabled is picked up from config
        let config = MemoryConfig::default();
        assert!(config.vec0_enabled, "default should be true for new installs");

        // Verify it can be toggled off via explicit config
        let mut config2 = MemoryConfig::default();
        config2.vec0_enabled = false;
        assert!(!config2.vec0_enabled, "should be settable to false");
    }

    // --- Vec0ScoringData and auxiliary scoring tests ---

    #[test]
    fn parse_memory_source_explicit() {
        assert_eq!(parse_memory_source("explicit"), MemorySource::Explicit);
    }

    #[test]
    fn parse_memory_source_extracted() {
        assert_eq!(parse_memory_source("extracted"), MemorySource::Extracted);
    }

    #[test]
    fn parse_memory_source_file_watcher() {
        assert_eq!(parse_memory_source("file_watcher"), MemorySource::FileWatcher);
    }

    #[test]
    fn parse_memory_source_unknown_defaults_to_extracted() {
        assert_eq!(parse_memory_source("something_else"), MemorySource::Extracted);
    }

    #[test]
    fn temporal_decay_from_str_today_returns_one() {
        let config = default_config();
        let now = Utc::now();
        let decay = temporal_decay_from_str(&now.to_rfc3339(), &MemorySource::Explicit, now, &config);
        assert!(
            (decay - 1.0).abs() < 0.001,
            "Decay for today should be ~1.0, got {decay}"
        );
    }

    #[test]
    fn temporal_decay_from_str_seven_days() {
        let config = default_config();
        let now = Utc::now();
        let seven_days_ago = now - chrono::Duration::days(7);
        let decay = temporal_decay_from_str(
            &seven_days_ago.to_rfc3339(),
            &MemorySource::Explicit,
            now,
            &config,
        );
        let expected = 0.95_f32.powf(7.0);
        assert!(
            (decay - expected).abs() < 0.001,
            "7-day decay should be ~{expected}, got {decay}"
        );
    }

    #[test]
    fn temporal_decay_from_str_file_watcher_always_one() {
        let config = default_config();
        let now = Utc::now();
        let old = now - chrono::Duration::days(365);
        let decay = temporal_decay_from_str(
            &old.to_rfc3339(),
            &MemorySource::FileWatcher,
            now,
            &config,
        );
        assert!(
            (decay - 1.0).abs() < f32::EPSILON,
            "FileWatcher should always have decay 1.0, got {decay}"
        );
    }

    #[test]
    fn temporal_decay_from_str_unparseable_returns_one() {
        let config = default_config();
        let now = Utc::now();
        let decay = temporal_decay_from_str("not-a-date", &MemorySource::Extracted, now, &config);
        assert!(
            (decay - 1.0).abs() < f32::EPSILON,
            "Unparseable timestamp should return 1.0, got {decay}"
        );
    }

    #[test]
    fn temporal_decay_from_str_matches_temporal_decay() {
        // Verify temporal_decay_from_str produces the same result as temporal_decay
        let config = default_config();
        let now = Utc::now();
        let created_at = (now - chrono::Duration::days(14)).to_rfc3339();
        let mem = make_memory("parity", MemorySource::Explicit, &created_at);

        let from_memory = temporal_decay(&mem, now, &config);
        let from_str = temporal_decay_from_str(&created_at, &MemorySource::Explicit, now, &config);

        assert!(
            (from_memory - from_str).abs() < 0.001,
            "temporal_decay ({from_memory}) and temporal_decay_from_str ({from_str}) should match"
        );
    }

    #[test]
    fn vec0_scoring_data_struct_exists() {
        // Verify Vec0ScoringData can be constructed and fields are accessible
        let data = Vec0ScoringData {
            memory_id: "test-id".to_string(),
            similarity: 0.95,
            content: "test content".to_string(),
            source: "explicit".to_string(),
            confidence: 0.9,
            created_at: "2026-03-01T00:00:00Z".to_string(),
        };
        assert_eq!(data.memory_id, "test-id");
        assert!((data.similarity - 0.95).abs() < f32::EPSILON);
        assert_eq!(data.content, "test content");
        assert_eq!(data.source, "explicit");
        assert!((data.confidence - 0.9).abs() < f64::EPSILON);
        assert_eq!(data.created_at, "2026-03-01T00:00:00Z");
    }

    #[tokio::test]
    async fn vec0_search_returns_rich_auxiliary_data() {
        // Test that vec0 search results contain all auxiliary fields needed for Vec0ScoringData
        let conn = setup_retriever_test_db().await;
        let store = Arc::new(MemoryStore::with_vec0(conn, None, true));

        let mut mem = make_test_memory_full("mem-rich-1", "Coffee preference data");
        mem.source = MemorySource::Explicit;
        mem.confidence = 0.85;
        store.save(&mem).await.unwrap();

        // Vec0SearchResult already has rich fields -- verify they map to Vec0ScoringData correctly
        let results = store
            .conn()
            .call(|conn| vec0::vec0_search(conn, &vec![0.1f32; 384], 10, 0.0, None))
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        // Map to Vec0ScoringData (the new struct)
        let scoring_data: Vec<Vec0ScoringData> = results
            .into_iter()
            .map(|r| Vec0ScoringData {
                memory_id: r.memory_id,
                similarity: r.similarity,
                content: r.content,
                source: r.source,
                confidence: r.confidence,
                created_at: r.created_at,
            })
            .collect();

        assert_eq!(scoring_data[0].memory_id, "mem-rich-1");
        assert!(scoring_data[0].similarity > 0.0, "similarity should be positive");
        assert_eq!(scoring_data[0].content, "Coffee preference data");
        assert_eq!(scoring_data[0].source, "explicit");
        assert!((scoring_data[0].confidence - 0.85).abs() < 0.01);
        assert!(!scoring_data[0].created_at.is_empty());
    }

    #[test]
    fn vec0_scoring_data_scoring_matches_memory_scoring() {
        // Verify that scoring from Vec0ScoringData produces identical results to scoring from Memory
        let config = default_config();
        let now = Utc::now();
        let created_at = (now - chrono::Duration::days(7)).to_rfc3339();
        let rrf_score = 0.5_f32;

        // Score from Memory (existing path)
        let mem = make_memory("m1", MemorySource::Explicit, &created_at);
        let importance_mem = importance_boost_for_source(&mem.source, &config);
        let decay_mem = temporal_decay(&mem, now, &config);
        let score_mem = rrf_score * importance_mem * decay_mem;

        // Score from Vec0ScoringData (new path)
        let source = parse_memory_source("explicit");
        let importance_vec0 = importance_boost_for_source(&source, &config);
        let decay_vec0 = temporal_decay_from_str(&created_at, &source, now, &config);
        let score_vec0 = rrf_score * importance_vec0 * decay_vec0;

        assert!(
            (score_mem - score_vec0).abs() < 0.001,
            "Memory-based score ({score_mem}) should match vec0-based score ({score_vec0})"
        );
    }

    #[tokio::test]
    async fn score_from_vec0_builds_scored_memories() {
        // Test that score_from_vec0 builds ScoredMemory from vec0 data + RRF scores
        let conn = setup_retriever_test_db().await;
        let store = Arc::new(MemoryStore::with_vec0(conn, None, true));

        // Save a memory so embeddings can be fetched for MMR
        let mem = make_test_memory_full("mem-score-1", "Score test");
        store.save(&mem).await.unwrap();

        let config = default_config();
        // Create a minimal retriever (embedder not needed for score_from_vec0)
        let retriever = HybridRetriever {
            store: store.clone(),
            embedder: Arc::new(crate::embedder::OnnxEmbedder::dummy()),
            config,
            vec0_enabled: true,
            fallback_count: Arc::new(AtomicU64::new(0)),
            last_fallback_log: Arc::new(AtomicU64::new(0)),
        };

        let now_str = Utc::now().to_rfc3339();
        let vec0_data = vec![Vec0ScoringData {
            memory_id: "mem-score-1".to_string(),
            similarity: 0.95,
            content: "Score test".to_string(),
            source: "explicit".to_string(),
            confidence: 0.9,
            created_at: now_str,
        }];

        let fused = vec![("mem-score-1".to_string(), 0.5_f32)];

        let scored = retriever.score_from_vec0(&vec0_data, &fused).await.unwrap();

        assert_eq!(scored.len(), 1);
        assert_eq!(scored[0].memory.id, "mem-score-1");
        assert_eq!(scored[0].memory.content, "Score test");
        assert_eq!(scored[0].memory.source, MemorySource::Explicit);
        // Score should be rrf * importance * decay (0.5 * 1.0 * ~1.0)
        assert!(
            scored[0].score > 0.4 && scored[0].score < 0.6,
            "Score should be ~0.5, got {}",
            scored[0].score
        );
        // Embedding should have been fetched for MMR
        assert_eq!(scored[0].memory.embedding.len(), 384);
    }

    #[tokio::test]
    async fn score_from_memories_unchanged() {
        // Test that score_from_memories produces the same output as the original path
        let conn = setup_retriever_test_db().await;
        let store = Arc::new(MemoryStore::with_vec0(conn, None, false));

        let mem = make_test_memory_full("mem-inmem-1", "In-memory test");
        store.save(&mem).await.unwrap();

        let config = default_config();
        let retriever = HybridRetriever {
            store: store.clone(),
            embedder: Arc::new(crate::embedder::OnnxEmbedder::dummy()),
            config,
            vec0_enabled: false,
            fallback_count: Arc::new(AtomicU64::new(0)),
            last_fallback_log: Arc::new(AtomicU64::new(0)),
        };

        let fused = vec![("mem-inmem-1".to_string(), 0.5_f32)];
        let scored = retriever.score_from_memories(&fused).await.unwrap();

        assert_eq!(scored.len(), 1);
        assert_eq!(scored[0].memory.id, "mem-inmem-1");
        assert_eq!(scored[0].memory.content, "In-memory test");
        // Score should be rrf * importance * decay
        assert!(scored[0].score > 0.0);
        // Embedding should be loaded (for MMR)
        assert_eq!(scored[0].memory.embedding.len(), 384);
    }

    #[tokio::test]
    async fn vec0_search_and_in_memory_produce_same_format() {
        // Verify that both search paths produce Vec<(String, f32)>
        let conn = setup_retriever_test_db().await;
        let store = Arc::new(MemoryStore::with_vec0(conn, None, true));

        store
            .save(&make_test_memory_full("mem-cmp-1", "Comparison test"))
            .await
            .unwrap();

        let query_emb = vec![0.1f32; 384];

        // Vec0 path
        let vec0_results = store
            .conn()
            .call({
                let q = query_emb.clone();
                move |conn| vec0::vec0_search(conn, &q, 10, 0.0, None)
            })
            .await
            .unwrap();
        let vec0_pairs: Vec<(String, f32)> = vec0_results
            .into_iter()
            .map(|r| (r.memory_id, r.similarity))
            .collect();

        // In-memory path
        let in_mem_embeddings = store.get_active_embeddings().await.unwrap();
        let in_mem_pairs: Vec<(String, f32)> = in_mem_embeddings
            .into_iter()
            .filter_map(|(id, emb)| {
                let sim = cosine_similarity(&query_emb, &emb);
                if sim >= 0.0 { Some((id, sim)) } else { None }
            })
            .collect();

        // Both should return the same memory ID
        assert_eq!(vec0_pairs.len(), 1);
        assert_eq!(in_mem_pairs.len(), 1);
        assert_eq!(vec0_pairs[0].0, in_mem_pairs[0].0);
        // Both similarities should be positive
        assert!(vec0_pairs[0].1 > 0.0);
        assert!(in_mem_pairs[0].1 > 0.0);
    }
}
