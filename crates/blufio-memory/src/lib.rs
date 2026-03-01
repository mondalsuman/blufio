// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Long-term memory system for the Blufio agent framework.
//!
//! Provides ONNX-based local embedding inference, SQLite storage with
//! hybrid search (vector similarity + BM25 via FTS5), LLM-based memory
//! extraction, and a ConditionalProvider for context injection.
//!
//! ## Architecture
//!
//! - **OnnxEmbedder**: Local ONNX model for 384-dim embedding inference
//! - **MemoryStore**: SQLite persistence with BLOB vectors and FTS5
//! - **ModelManager**: First-run model download from HuggingFace
//! - **HybridRetriever**: Vector + BM25 + RRF fusion search
//! - **MemoryExtractor**: LLM-based fact extraction from conversations
//! - **MemoryProvider**: ConditionalProvider for context injection
//! - **Types**: Memory, MemorySource, MemoryStatus, ScoredMemory

pub mod embedder;
pub mod extractor;
pub mod model_manager;
pub mod provider;
pub mod retriever;
pub mod store;
pub mod types;

pub use embedder::OnnxEmbedder;
pub use extractor::MemoryExtractor;
pub use model_manager::ModelManager;
pub use provider::MemoryProvider;
pub use retriever::HybridRetriever;
pub use store::MemoryStore;
pub use types::*;
