//! Semantic code chunker using tree-sitter AST parsing
//!
//! Architecture Decision: We chunk code by semantic units (functions, classes, methods)
//! rather than arbitrary line windows because:
//! 1. Embeddings work better on coherent semantic units
//! 2. Search results are more meaningful at function/class level
//! 3. We can fall back to sliding windows for non-parseable files

mod languages;

use anyhow::{Context, Result};
use std::path::Path;

pub use languages::Language;

/// A semantic chunk of code extracted from a file
#[derive(Debug, Clone)]
pub struct CodeChunk {
    /// Path to the source file
    pub file_path: String,
    /// Programming language
    pub language: Language,
    /// Type of chunk (function, class, method, block)
    pub chunk_type: ChunkType,
    /// Name of the symbol (if applicable)
    pub name: Option<String>,
    /// Starting byte offset
    pub start_byte: usize,
    /// Ending byte offset
    pub end_byte: usize,
    /// Starting line (1-indexed)
    pub start_line: usize,
    /// Ending line (1-indexed)
    pub end_line: usize,
    /// The actual code content
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkType {
    Function,
    Class,
    Method,
    Struct,
    Impl,
    Module,
    Block, // Fallback for sliding window
}

impl std::fmt::Display for ChunkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChunkType::Function => write!(f, "function"),
            ChunkType::Class => write!(f, "class"),
            ChunkType::Method => write!(f, "method"),
            ChunkType::Struct => write!(f, "struct"),
            ChunkType::Impl => write!(f, "impl"),
            ChunkType::Module => write!(f, "module"),
            ChunkType::Block => write!(f, "block"),
        }
    }
}

/// Main chunker that processes files into semantic units
pub struct Chunker {
    /// Maximum chunk size in bytes (for sliding window fallback)
    max_chunk_size: usize,
    /// Overlap ratio for sliding windows
    overlap_ratio: f32,
}

impl Default for Chunker {
    fn default() -> Self {
        Self {
            max_chunk_size: 2000, // ~500 tokens
            overlap_ratio: 0.2,   // 20% overlap
        }
    }
}

impl Chunker {
    pub fn new(max_chunk_size: usize, overlap_ratio: f32) -> Self {
        Self {
            max_chunk_size,
            overlap_ratio,
        }
    }

    /// Chunk a file into semantic units
    pub fn chunk_file(&self, file_path: &Path, content: &str) -> Result<Vec<CodeChunk>> {
        let language = Language::from_path(file_path);

        match language {
            Language::Unknown => self.chunk_sliding_window(file_path, content, language),
            _ => {
                // Try tree-sitter parsing first
                match self.chunk_with_tree_sitter(file_path, content, language) {
                    Ok(chunks) if !chunks.is_empty() => Ok(chunks),
                    _ => {
                        // Fall back to sliding window
                        tracing::debug!("Falling back to sliding window for {:?}", file_path);
                        self.chunk_sliding_window(file_path, content, language)
                    }
                }
            }
        }
    }

    /// Parse with tree-sitter and extract semantic chunks
    fn chunk_with_tree_sitter(
        &self,
        file_path: &Path,
        content: &str,
        language: Language,
    ) -> Result<Vec<CodeChunk>> {
        let mut parser = tree_sitter::Parser::new();
        let ts_language = language
            .tree_sitter_language()
            .context("Failed to get tree-sitter language")?;

        parser
            .set_language(&ts_language)
            .context("Failed to set parser language")?;

        let tree = parser
            .parse(content, None)
            .context("Failed to parse file")?;

        let mut chunks = Vec::new();
        let file_path_str = file_path.to_string_lossy().to_string();

        // Walk the AST and extract relevant nodes
        self.extract_chunks_recursive(
            tree.root_node(),
            content,
            &file_path_str,
            language,
            &mut chunks,
        );

        Ok(chunks)
    }

    /// Recursively extract chunks from AST nodes
    fn extract_chunks_recursive(
        &self,
        node: tree_sitter::Node,
        content: &str,
        file_path: &str,
        language: Language,
        chunks: &mut Vec<CodeChunk>,
    ) {
        let chunk_type = self.node_to_chunk_type(node.kind(), language);

        if let Some(chunk_type) = chunk_type {
            let start_byte = node.start_byte();
            let end_byte = node.end_byte();
            let chunk_content = &content[start_byte..end_byte];

            // Skip very small chunks (less than 50 bytes)
            if chunk_content.len() >= 50 {
                let name = self.extract_name(node, content, language);

                chunks.push(CodeChunk {
                    file_path: file_path.to_string(),
                    language,
                    chunk_type,
                    name,
                    start_byte,
                    end_byte,
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    content: chunk_content.to_string(),
                });
            }
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_chunks_recursive(child, content, file_path, language, chunks);
        }
    }

    /// Map AST node kind to chunk type based on language
    fn node_to_chunk_type(&self, kind: &str, language: Language) -> Option<ChunkType> {
        match language {
            Language::Rust => match kind {
                "function_item" => Some(ChunkType::Function),
                "impl_item" => Some(ChunkType::Impl),
                "struct_item" => Some(ChunkType::Struct),
                "mod_item" => Some(ChunkType::Module),
                _ => None,
            },
            Language::Python => match kind {
                "function_definition" => Some(ChunkType::Function),
                "class_definition" => Some(ChunkType::Class),
                _ => None,
            },
            Language::JavaScript | Language::TypeScript => match kind {
                "function_declaration" | "arrow_function" | "function" => Some(ChunkType::Function),
                "class_declaration" => Some(ChunkType::Class),
                "method_definition" => Some(ChunkType::Method),
                _ => None,
            },
            Language::Go => match kind {
                "function_declaration" | "method_declaration" => Some(ChunkType::Function),
                "type_declaration" => Some(ChunkType::Struct),
                _ => None,
            },
            Language::Unknown => None,
        }
    }

    /// Extract the name of a function/class/method from the AST
    fn extract_name(
        &self,
        node: tree_sitter::Node,
        content: &str,
        language: Language,
    ) -> Option<String> {
        // Find the identifier child node
        let name_field = match language {
            Language::Rust => "name",
            Language::Python => "name",
            Language::JavaScript | Language::TypeScript => "name",
            Language::Go => "name",
            Language::Unknown => return None,
        };

        node.child_by_field_name(name_field)
            .map(|n| content[n.start_byte()..n.end_byte()].to_string())
    }

    /// Fallback: chunk using sliding window with overlap
    fn chunk_sliding_window(
        &self,
        file_path: &Path,
        content: &str,
        language: Language,
    ) -> Result<Vec<CodeChunk>> {
        let mut chunks = Vec::new();
        let file_path_str = file_path.to_string_lossy().to_string();
        let _lines: Vec<&str> = content.lines().collect();

        if content.is_empty() {
            return Ok(chunks);
        }

        let overlap = (self.max_chunk_size as f32 * self.overlap_ratio) as usize;
        let step = self.max_chunk_size - overlap;

        let mut start = 0;
        let mut chunk_num = 0;

        while start < content.len() {
            let end = (start + self.max_chunk_size).min(content.len());

            // Find the start and end lines
            let start_line = content[..start].matches('\n').count() + 1;
            let end_line = content[..end].matches('\n').count() + 1;

            chunks.push(CodeChunk {
                file_path: file_path_str.clone(),
                language,
                chunk_type: ChunkType::Block,
                name: Some(format!("block_{}", chunk_num)),
                start_byte: start,
                end_byte: end,
                start_line,
                end_line,
                content: content[start..end].to_string(),
            });

            chunk_num += 1;
            start += step;

            // Avoid tiny trailing chunks
            if content.len() - start < self.max_chunk_size / 4 {
                break;
            }
        }

        Ok(chunks)
    }
}
