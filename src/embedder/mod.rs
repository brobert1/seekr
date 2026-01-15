//! Local embedding generation using fastembed-rs
//!
//! Using fastembed with BGE-small model (384 dimensions) for fast,
//! local semantic embeddings via ONNX Runtime.

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

/// Wrapper around fastembed for generating text embeddings
pub struct Embedder {
    model: TextEmbedding,
}

impl Embedder {
    /// Create a new embedder with the default model (BGE-small-en-v1.5)
    pub fn new() -> Result<Self> {
        tracing::info!("Loading embedding model (bge-small-en-v1.5)...");

        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(true),
        )
        .context("Failed to initialize embedding model")?;

        tracing::info!("Embedding model loaded successfully");

        Ok(Self { model })
    }

    /// Generate embedding for a single text
    pub fn embed_one(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self
            .model
            .embed(vec![text], None)
            .context("Failed to generate embedding")?;

        embeddings
            .into_iter()
            .next()
            .context("No embedding generated")
    }

    /// Generate embeddings for multiple texts (batched for efficiency)
    pub fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        self.model
            .embed(texts, None)
            .context("Failed to generate batch embeddings")
    }
}

impl Default for Embedder {
    fn default() -> Self {
        Self::new().expect("Failed to create default embedder")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_dimension() {
        let embedder = Embedder::new().unwrap();
        let embedding = embedder.embed_one("test code").unwrap();
        assert_eq!(embedding.len(), 384);
    }
}
