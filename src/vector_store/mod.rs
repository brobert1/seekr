//! Vector store using usearch for fast approximate nearest neighbor search
//!
//! Architecture Decision: Using usearch because:
//! - Lightweight and fast (no external services)
//! - Supports multiple distance metrics (cosine, L2)
//! - Memory-mapped for efficient large-scale search
//! - Native Rust bindings

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use usearch::{new_index, Index, IndexOptions, MetricKind, ScalarKind};

/// Metadata stored alongside each vector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub file_path: String,
    pub chunk_type: String,
    pub name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub language: String,
    pub content_preview: String, // First 200 chars for display
}

/// Vector store for semantic search
pub struct VectorStore {
    index: Index,
    index_path: PathBuf,
    metadata_path: PathBuf,
    metadata: Vec<ChunkMetadata>,
    dimension: usize,
}

impl VectorStore {
    /// Create or open a vector store at the given path
    pub fn new(base_path: &Path, dimension: usize) -> Result<Self> {
        let index_path = base_path.join("vectors.usearch");
        let metadata_path = base_path.join("metadata.json");

        fs::create_dir_all(base_path)?;

        let options = IndexOptions {
            dimensions: dimension,
            metric: MetricKind::Cos, // Cosine similarity for text embeddings
            quantization: ScalarKind::F32,
            connectivity: 16,        // M parameter for HNSW
            expansion_add: 128,      // ef_construction
            expansion_search: 64,    // ef_search
            multi: false,
        };

        let index = new_index(&options).context("Failed to create vector index")?;

        // Load existing metadata if present
        let metadata = if metadata_path.exists() {
            let data = fs::read_to_string(&metadata_path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };

        // Load existing index if present
        if index_path.exists() {
            index.load(index_path.to_str().unwrap())
                .context("Failed to load existing index")?;
        }

        Ok(Self {
            index,
            index_path,
            metadata_path,
            metadata,
            dimension,
        })
    }

    /// Add a vector with its metadata
    pub fn add(&mut self, vector: &[f32], metadata: ChunkMetadata) -> Result<u64> {
        let key = self.metadata.len() as u64;
        
        // Ensure index has capacity (usearch requires this)
        let current_capacity = self.index.capacity();
        if key >= current_capacity as u64 {
            let new_capacity = (current_capacity + 1000).max(1000);
            self.index.reserve(new_capacity)
                .context("Failed to reserve index capacity")?;
        }
        
        self.index
            .add(key, vector)
            .context("Failed to add vector to index")?;
        
        self.metadata.push(metadata);
        
        Ok(key)
    }

    /// Add multiple vectors in batch
    pub fn add_batch(&mut self, vectors: &[Vec<f32>], metadatas: Vec<ChunkMetadata>) -> Result<()> {
        for (vector, meta) in vectors.iter().zip(metadatas) {
            self.add(vector, meta)?;
        }
        Ok(())
    }

    /// Search for similar vectors
    pub fn search(&self, query_vector: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        let results = self.index
            .search(query_vector, limit)
            .context("Failed to search vectors")?;

        let mut search_results = Vec::new();
        
        for (key, distance) in results.keys.iter().zip(results.distances.iter()) {
            let key = *key as usize;
            if key < self.metadata.len() {
                search_results.push(SearchResult {
                    score: 1.0 - distance, // Convert distance to similarity
                    metadata: self.metadata[key].clone(),
                });
            }
        }

        Ok(search_results)
    }

    /// Save the index and metadata to disk
    pub fn save(&self) -> Result<()> {
        self.index
            .save(self.index_path.to_str().unwrap())
            .context("Failed to save vector index")?;

        let metadata_json = serde_json::to_string_pretty(&self.metadata)?;
        fs::write(&self.metadata_path, metadata_json)?;

        Ok(())
    }

    /// Get the number of vectors in the store
    pub fn len(&self) -> usize {
        self.index.size()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all vectors and metadata
    pub fn clear(&mut self) -> Result<()> {
        // Create a fresh index
        let options = IndexOptions {
            dimensions: self.dimension,
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            connectivity: 16,
            expansion_add: 128,
            expansion_search: 64,
            multi: false,
        };

        self.index = new_index(&options)?;
        self.metadata.clear();

        // Remove files
        if self.index_path.exists() {
            fs::remove_file(&self.index_path)?;
        }
        if self.metadata_path.exists() {
            fs::remove_file(&self.metadata_path)?;
        }

        Ok(())
    }
}

/// A search result from the vector store
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub score: f32,
    pub metadata: ChunkMetadata,
}
