//! Local embedding generation using fastembed-rs
//!
//! Architecture Decision: Using fastembed with BGE-small model because:
//! - 384 dimensions = faster similarity search
//! - Int8 quantization available = smaller memory footprint
//! - Good performance on code understanding tasks
//! - Runs entirely locally via ONNX Runtime

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

/// Wrapper around fastembed for generating text embeddings
pub struct Embedder {
    model: TextEmbedding,
    dimension: usize,
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

        Ok(Self {
            model,
            dimension: 384, // BGE-small-en-v1.5 dimension
        })
    }

    /// Create embedder with a specific model
    pub fn with_model(model_name: EmbeddingModel) -> Result<Self> {
        let dimension = match model_name {
            EmbeddingModel::BGESmallENV15 => 384,
            EmbeddingModel::BGEBaseENV15 => 768,
            EmbeddingModel::BGELargeENV15 => 1024,
            EmbeddingModel::AllMiniLML6V2 => 384,
            EmbeddingModel::AllMiniLML12V2 => 384,
            _ => 384, // Default fallback
        };

        let model =
            TextEmbedding::try_new(InitOptions::new(model_name).with_show_download_progress(true))
                .context("Failed to initialize embedding model")?;

        Ok(Self { model, dimension })
    }

    /// Get the embedding dimension
    pub fn dimension(&self) -> usize {
        self.dimension
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

    /// Generate embedding for a code chunk with context
    /// Adds language and type information for better embeddings
    pub fn embed_code_chunk(
        &self,
        code: &str,
        language: &str,
        chunk_type: &str,
        name: Option<&str>,
    ) -> Result<Vec<f32>> {
        // Create a contextualized representation
        let context = match name {
            Some(n) => format!("[{}] {} {}: {}", language, chunk_type, n, code),
            None => format!("[{}] {}: {}", language, chunk_type, code),
        };

        self.embed_one(&context)
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
