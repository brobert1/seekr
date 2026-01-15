//! Semantic indexer that combines chunking, embedding, and vector storage
//!
//! This module orchestrates the Phase 2 components:
//! 1. Chunker: Parse code into semantic units (functions, classes)
//! 2. Embedder: Generate vector embeddings for each chunk
//! 3. VectorStore: Store and search embeddings efficiently

use anyhow::{Context, Result};
use std::path::Path;
use std::time::Instant;

use crate::chunker::{Chunker, CodeChunk};
use crate::embedder::Embedder;
use crate::vector_store::{ChunkMetadata, VectorStore};

/// Statistics from semantic indexing
#[derive(Debug, Default)]
pub struct SemanticIndexStats {
    pub files_processed: usize,
    pub chunks_created: usize,
    pub embeddings_generated: usize,
    pub duration_secs: f64,
}

/// Semantic search result
#[derive(Debug, Clone)]
pub struct SemanticResult {
    pub file_path: String,
    pub chunk_type: String,
    pub name: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub language: String,
    pub content_preview: String,
    pub similarity_score: f32,
}

/// Combined semantic indexer
pub struct SemanticIndexer {
    chunker: Chunker,
    embedder: Option<Embedder>,
    vector_store: Option<VectorStore>,
    index_path: std::path::PathBuf,
}

impl SemanticIndexer {
    /// Create a new semantic indexer
    pub fn new(base_path: &Path) -> Result<Self> {
        let index_path = base_path.join("semantic");
        std::fs::create_dir_all(&index_path)?;

        Ok(Self {
            chunker: Chunker::default(),
            embedder: None,
            vector_store: None,
            index_path,
        })
    }

    /// Initialize the embedder (lazy loading for faster startup)
    fn ensure_embedder(&mut self) -> Result<&Embedder> {
        if self.embedder.is_none() {
            self.embedder = Some(Embedder::new()?);
        }
        Ok(self.embedder.as_ref().unwrap())
    }

    /// Initialize the vector store
    fn ensure_vector_store(&mut self) -> Result<&mut VectorStore> {
        if self.vector_store.is_none() {
            let dimension = 384; // BGE-small dimension
            self.vector_store = Some(VectorStore::new(&self.index_path, dimension)?);
        }
        Ok(self.vector_store.as_mut().unwrap())
    }

    /// Index all files from specified paths
    pub fn index_files<P: AsRef<Path>>(
        &mut self,
        files: &[(P, String)],
    ) -> Result<SemanticIndexStats> {
        let start = Instant::now();
        let mut stats = SemanticIndexStats::default();

        // Collect all chunks first
        let mut all_chunks: Vec<CodeChunk> = Vec::new();

        for (path, content) in files {
            let path = path.as_ref();
            match self.chunker.chunk_file(path, content) {
                Ok(chunks) => {
                    stats.files_processed += 1;
                    all_chunks.extend(chunks);
                }
                Err(e) => {
                    tracing::debug!("Failed to chunk {:?}: {}", path, e);
                }
            }
        }

        stats.chunks_created = all_chunks.len();

        if all_chunks.is_empty() {
            stats.duration_secs = start.elapsed().as_secs_f64();
            return Ok(stats);
        }

        // Initialize embedder and vector store
        self.ensure_embedder()?;
        self.ensure_vector_store()?;
        let embedder = self.embedder.as_ref().unwrap();
        let store = self.vector_store.as_mut().unwrap();

        // Process in batches of 32 to limit memory usage
        const BATCH_SIZE: usize = 32;
        let total_batches = (all_chunks.len() + BATCH_SIZE - 1) / BATCH_SIZE;

        for (batch_idx, chunk_batch) in all_chunks.chunks(BATCH_SIZE).enumerate() {
            // Progress indicator
            print!(
                "\r   Processing batch {}/{} ({} chunks)...   ",
                batch_idx + 1,
                total_batches,
                stats.embeddings_generated + chunk_batch.len()
            );
            std::io::Write::flush(&mut std::io::stdout()).ok();

            // Prepare texts for this batch
            let texts: Vec<String> = chunk_batch
                .iter()
                .map(|chunk| {
                    format!(
                        "[{}] {}: {}",
                        chunk.language.name(),
                        chunk.chunk_type,
                        &chunk.content[..chunk.content.len().min(500)] // Limit chunk size
                    )
                })
                .collect();

            let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            
            // Embed this batch
            let embeddings = embedder.embed_batch(text_refs)?;
            stats.embeddings_generated += embeddings.len();

            // Store embeddings immediately (don't hold in memory)
            for (chunk, embedding) in chunk_batch.iter().zip(embeddings.iter()) {
                let metadata = ChunkMetadata {
                    file_path: chunk.file_path.clone(),
                    chunk_type: chunk.chunk_type.to_string(),
                    name: chunk.name.clone(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    language: chunk.language.name().to_string(),
                    content_preview: chunk.content.chars().take(200).collect(),
                };

                store.add(embedding, metadata)?;
            }
        }

        println!(); // Newline after progress
        store.save()?;
        stats.duration_secs = start.elapsed().as_secs_f64();

        Ok(stats)
    }

    /// Search for semantically similar code
    pub fn search(&mut self, query: &str, limit: usize) -> Result<Vec<SemanticResult>> {
        self.ensure_embedder()?;
        self.ensure_vector_store()?;

        let embedder = self.embedder.as_ref().unwrap();
        let store = self.vector_store.as_ref().unwrap();

        // Embed the query
        let query_embedding = embedder.embed_one(query)?;

        // Search vector store
        let results = store.search(&query_embedding, limit)?;

        Ok(results
            .into_iter()
            .map(|r| SemanticResult {
                file_path: r.metadata.file_path,
                chunk_type: r.metadata.chunk_type,
                name: r.metadata.name,
                start_line: r.metadata.start_line,
                end_line: r.metadata.end_line,
                language: r.metadata.language,
                content_preview: r.metadata.content_preview,
                similarity_score: r.score,
            })
            .collect())
    }

    /// Check if semantic index exists
    pub fn index_exists(&self) -> bool {
        self.index_path.join("vectors.usearch").exists()
    }

    /// Get index statistics
    pub fn get_stats(&mut self) -> Result<(usize, u64)> {
        self.ensure_vector_store()?;
        let store = self.vector_store.as_ref().unwrap();

        let num_vectors = store.len();
        let size = std::fs::metadata(self.index_path.join("vectors.usearch"))
            .map(|m| m.len())
            .unwrap_or(0);

        Ok((num_vectors, size))
    }
}
