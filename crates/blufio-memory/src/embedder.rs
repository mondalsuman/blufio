// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! ONNX embedding adapter for local inference using all-MiniLM-L6-v2.
//!
//! Produces 384-dimensional embeddings on CPU with zero external API calls.

use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use ndarray::Array2;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::TensorRef;

use blufio_core::error::BlufioError;
use blufio_core::traits::adapter::PluginAdapter;
use blufio_core::traits::EmbeddingAdapter;
use blufio_core::types::{AdapterType, EmbeddingInput, EmbeddingOutput, HealthStatus};

/// Embedding dimensions for all-MiniLM-L6-v2.
pub const EMBEDDING_DIM: usize = 384;

/// ONNX-based embedding adapter using all-MiniLM-L6-v2.
///
/// Loads the quantized INT8 ONNX model and tokenizer from disk.
/// All inference runs on CPU with a single thread (optimized for VPS).
pub struct OnnxEmbedder {
    /// ONNX Runtime session (not Send, wrapped in Mutex for safety).
    session: Mutex<Session>,
    /// HuggingFace tokenizer.
    tokenizer: tokenizers::Tokenizer,
}

// Safety: Session is accessed through Mutex which provides synchronization.
// The tokenizer is thread-safe for encoding operations.
unsafe impl Send for OnnxEmbedder {}
unsafe impl Sync for OnnxEmbedder {}

impl OnnxEmbedder {
    /// Creates a new ONNX embedder from model files on disk.
    ///
    /// Expects `model.onnx` and `tokenizer.json` in the same directory
    /// as the provided model path (or its parent).
    pub fn new(model_path: &Path) -> Result<Self, BlufioError> {
        let model_dir = model_path
            .parent()
            .ok_or_else(|| BlufioError::Internal("Invalid model path".to_string()))?;

        let tokenizer_path = model_dir.join("tokenizer.json");
        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            BlufioError::Internal(format!(
                "Failed to load tokenizer from {}: {e}",
                tokenizer_path.display()
            ))
        })?;

        let session = Session::builder()
            .map_err(|e| BlufioError::Internal(format!("Failed to create ONNX session builder: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| BlufioError::Internal(format!("Failed to set optimization level: {e}")))?
            .with_intra_threads(1)
            .map_err(|e| BlufioError::Internal(format!("Failed to set thread count: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| {
                BlufioError::Internal(format!(
                    "Failed to load ONNX model from {}: {e}",
                    model_path.display()
                ))
            })?;

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
        })
    }

    /// Embed a single text string, returning a 384-dim f32 vector.
    pub fn embed_text(&self, text: &str) -> Result<Vec<f32>, BlufioError> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| BlufioError::Internal(format!("Tokenization failed: {e}")))?;

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&m| m as i64)
            .collect();
        let token_type_ids: Vec<i64> = encoding
            .get_type_ids()
            .iter()
            .map(|&t| t as i64)
            .collect();

        let seq_len = input_ids.len();

        let input_ids_array =
            Array2::from_shape_vec((1, seq_len), input_ids).map_err(|e| {
                BlufioError::Internal(format!("Failed to create input_ids tensor: {e}"))
            })?;
        let attention_mask_array =
            Array2::from_shape_vec((1, seq_len), attention_mask.clone()).map_err(|e| {
                BlufioError::Internal(format!("Failed to create attention_mask tensor: {e}"))
            })?;
        let token_type_ids_array =
            Array2::from_shape_vec((1, seq_len), token_type_ids).map_err(|e| {
                BlufioError::Internal(format!("Failed to create token_type_ids tensor: {e}"))
            })?;

        let mut session = self.session.lock().map_err(|e| {
            BlufioError::Internal(format!("Failed to lock ONNX session: {e}"))
        })?;

        let input_ids_tensor =
            TensorRef::from_array_view(&input_ids_array).map_err(|e| {
                BlufioError::Internal(format!("Failed to create input_ids TensorRef: {e}"))
            })?;
        let attention_mask_tensor =
            TensorRef::from_array_view(&attention_mask_array).map_err(|e| {
                BlufioError::Internal(format!("Failed to create attention_mask TensorRef: {e}"))
            })?;
        let token_type_ids_tensor =
            TensorRef::from_array_view(&token_type_ids_array).map_err(|e| {
                BlufioError::Internal(format!("Failed to create token_type_ids TensorRef: {e}"))
            })?;

        let outputs = session
            .run(ort::inputs![
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
                "token_type_ids" => token_type_ids_tensor
            ])
            .map_err(|e| BlufioError::Internal(format!("ONNX inference failed: {e}")))?;

        // Extract output: shape [1, seq_len, 384]
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| BlufioError::Internal(format!("Failed to extract output tensor: {e}")))?;

        // Apply attention-masked mean pooling
        let hidden_size = shape[shape.len() - 1] as usize;
        let pooled = mean_pool_with_attention(data, &attention_mask, seq_len, hidden_size);

        // L2 normalize
        let normalized = l2_normalize(&pooled);

        Ok(normalized)
    }
}

/// Apply attention-masked mean pooling over token embeddings.
fn mean_pool_with_attention(
    embeddings: &[f32],
    attention_mask: &[i64],
    seq_len: usize,
    hidden_size: usize,
) -> Vec<f32> {
    let mut sum = vec![0.0f32; hidden_size];
    let mut count = 0.0f32;

    for i in 0..seq_len {
        if attention_mask[i] > 0 {
            for j in 0..hidden_size {
                sum[j] += embeddings[i * hidden_size + j];
            }
            count += 1.0;
        }
    }

    if count > 0.0 {
        for val in &mut sum {
            *val /= count;
        }
    }

    sum
}

/// L2-normalize a vector.
fn l2_normalize(vec: &[f32]) -> Vec<f32> {
    let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        vec.iter().map(|v| v / norm).collect()
    } else {
        vec.to_vec()
    }
}

#[async_trait]
impl PluginAdapter for OnnxEmbedder {
    fn name(&self) -> &str {
        "onnx-embedder"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Embedding
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        // Try to lock the session to verify it's alive
        match self.session.lock() {
            Ok(_) => Ok(HealthStatus::Healthy),
            Err(e) => Ok(HealthStatus::Unhealthy(format!("Session lock poisoned: {e}"))),
        }
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        Ok(())
    }
}

#[async_trait]
impl EmbeddingAdapter for OnnxEmbedder {
    async fn embed(&self, input: EmbeddingInput) -> Result<EmbeddingOutput, BlufioError> {
        let mut embeddings = Vec::with_capacity(input.texts.len());

        for text in &input.texts {
            let vec = self.embed_text(text)?;
            embeddings.push(vec);
        }

        Ok(EmbeddingOutput {
            embeddings,
            dimensions: EMBEDDING_DIM,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l2_normalize_unit_vector() {
        let v = vec![1.0, 0.0, 0.0];
        let n = l2_normalize(&v);
        assert!((n[0] - 1.0).abs() < f32::EPSILON);
        assert!(n[1].abs() < f32::EPSILON);
        assert!(n[2].abs() < f32::EPSILON);
    }

    #[test]
    fn l2_normalize_general_vector() {
        let v = vec![3.0, 4.0];
        let n = l2_normalize(&v);
        // norm = 5, so normalized = [0.6, 0.8]
        assert!((n[0] - 0.6).abs() < 0.001);
        assert!((n[1] - 0.8).abs() < 0.001);

        // Verify unit length
        let norm: f32 = n.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[test]
    fn l2_normalize_zero_vector() {
        let v = vec![0.0, 0.0, 0.0];
        let n = l2_normalize(&v);
        assert_eq!(n, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn mean_pool_with_attention_basic() {
        // 2 tokens, hidden_size=3, first token masked out (padding)
        let embeddings = vec![
            0.0, 0.0, 0.0, // token 0 (padding)
            1.0, 2.0, 3.0, // token 1 (real)
        ];
        let attention_mask = vec![0, 1];
        let result = mean_pool_with_attention(&embeddings, &attention_mask, 2, 3);
        assert_eq!(result, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn mean_pool_with_attention_multiple() {
        // 3 tokens, hidden_size=2, all real
        let embeddings = vec![
            1.0, 2.0, // token 0
            3.0, 4.0, // token 1
            5.0, 6.0, // token 2
        ];
        let attention_mask = vec![1, 1, 1];
        let result = mean_pool_with_attention(&embeddings, &attention_mask, 3, 2);
        assert!((result[0] - 3.0).abs() < f32::EPSILON); // mean of 1,3,5
        assert!((result[1] - 4.0).abs() < f32::EPSILON); // mean of 2,4,6
    }

    // Note: OnnxEmbedder::new requires actual model files.
    // Integration tests with model download are done separately.
    // The EmbeddingAdapter trait implementation is verified at compile time.
}
