//! Tantivy-based BM25 indexer for code files
//!
//! Architecture Decision: Using Tantivy for lexical search because:
//! - Best-in-class BM25 implementation in Rust
//! - Fast incremental updates
//! - Excellent memory efficiency
//! - Supports custom tokenizers for code

mod schema;

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy};

pub use schema::SearchResult;

/// Statistics from an indexing operation
#[derive(Debug, Default)]
pub struct IndexStats {
    pub files_indexed: usize,
    pub total_lines: usize,
    pub duration_secs: f64,
}

/// Index health status
#[derive(Debug)]
pub struct IndexStatus {
    pub num_docs: u64,
    pub size_bytes: u64,
    pub healthy: bool,
}

/// Main indexer wrapping Tantivy
pub struct Indexer {
    index: Index,
    schema: Schema,
    reader: Option<IndexReader>,
}

impl Indexer {
    /// Get the default index path (~/.seekr/index)
    pub fn default_index_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not find home directory")?;
        Ok(home.join(".seekr").join("index"))
    }

    /// Create a new indexer (creates/overwrites index)
    pub fn new(workspace_path: &Path, force: bool) -> Result<Self> {
        let index_path = Self::default_index_path()?;

        // Remove existing index if force or doesn't exist
        if force && index_path.exists() {
            fs::remove_dir_all(&index_path)?;
        }
        fs::create_dir_all(&index_path)?;

        let schema = schema::build_schema();
        let index = Index::create_in_dir(&index_path, schema.clone())
            .or_else(|_| Index::open_in_dir(&index_path))?;

        // Store workspace path in index metadata
        let meta_path = index_path.join("workspace.txt");
        fs::write(meta_path, workspace_path.to_string_lossy().as_bytes())?;

        Ok(Self {
            index,
            schema,
            reader: None,
        })
    }

    /// Open an existing index for searching
    pub fn open(index_path: &Path) -> Result<Self> {
        let index = Index::open_in_dir(index_path)?;
        let schema = index.schema();
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            schema,
            reader: Some(reader),
        })
    }

    /// Get index status
    pub fn get_status(index_path: &Path) -> Result<IndexStatus> {
        let index = Index::open_in_dir(index_path)?;
        let reader = index.reader()?;
        let searcher = reader.searcher();

        // Calculate total size
        let size_bytes = fs::read_dir(index_path)?
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum();

        Ok(IndexStatus {
            num_docs: searcher.num_docs(),
            size_bytes,
            healthy: true,
        })
    }

    /// Index all files in a directory
    pub fn index_directory(&mut self, path: &Path) -> Result<IndexStats> {
        let start = Instant::now();
        let mut stats = IndexStats::default();

        let mut writer: IndexWriter = self.index.writer(50_000_000)?; // 50MB heap

        // Use ignore crate to respect .gitignore
        let walker = WalkBuilder::new(path)
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        let file_path_field = self.schema.get_field("file_path").unwrap();
        let content_field = self.schema.get_field("content").unwrap();
        let language_field = self.schema.get_field("language").unwrap();
        let line_count_field = self.schema.get_field("line_count").unwrap();

        for entry in walker.filter_map(|e| e.ok()) {
            let entry_path = entry.path();

            // Skip directories and non-text files
            if !entry_path.is_file() {
                continue;
            }

            // Detect language from extension
            let language = match entry_path.extension().and_then(|e| e.to_str()) {
                Some("rs") => "rust",
                Some("py") => "python",
                Some("ts" | "tsx") => "typescript",
                Some("js" | "jsx") => "javascript",
                Some("go") => "go",
                Some("java") => "java",
                Some("c" | "h") => "c",
                Some("cpp" | "hpp" | "cc") => "cpp",
                Some("rb") => "ruby",
                Some("md") => "markdown",
                Some("toml") => "toml",
                Some("yaml" | "yml") => "yaml",
                Some("json") => "json",
                _ => continue, // Skip unsupported file types
            };

            // Read file content
            let content = match fs::read_to_string(entry_path) {
                Ok(c) => c,
                Err(_) => continue, // Skip binary/unreadable files
            };

            let line_count = content.lines().count();
            let relative_path = entry_path
                .strip_prefix(path)
                .unwrap_or(entry_path)
                .to_string_lossy();

            writer.add_document(doc!(
                file_path_field => relative_path.to_string(),
                content_field => content,
                language_field => language,
                line_count_field => line_count as u64
            ))?;

            stats.files_indexed += 1;
            stats.total_lines += line_count;
        }

        writer.commit()?;
        stats.duration_secs = start.elapsed().as_secs_f64();

        Ok(stats)
    }

    /// Incrementally index only changed files
    pub fn index_directory_incremental(
        &mut self,
        path: &Path,
        cache: &mut crate::cache::FileCache,
    ) -> Result<IndexStats> {
        use crate::cache::FileStatus;
        
        let start = Instant::now();
        let mut stats = IndexStats::default();
        let mut changed_files = 0;
        let mut skipped_files = 0;

        let mut writer: IndexWriter = self.index.writer(50_000_000)?;

        let walker = WalkBuilder::new(path)
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        let file_path_field = self.schema.get_field("file_path").unwrap();
        let content_field = self.schema.get_field("content").unwrap();
        let language_field = self.schema.get_field("language").unwrap();
        let line_count_field = self.schema.get_field("line_count").unwrap();

        for entry in walker.filter_map(|e| e.ok()) {
            let entry_path = entry.path();

            if !entry_path.is_file() {
                continue;
            }

            let language = match entry_path.extension().and_then(|e| e.to_str()) {
                Some("rs") => "rust",
                Some("py") => "python",
                Some("ts" | "tsx") => "typescript",
                Some("js" | "jsx") => "javascript",
                Some("go") => "go",
                Some("java") => "java",
                Some("c" | "h") => "c",
                Some("cpp" | "hpp" | "cc") => "cpp",
                Some("rb") => "ruby",
                Some("md") => "markdown",
                Some("toml") => "toml",
                Some("yaml" | "yml") => "yaml",
                Some("json") => "json",
                _ => continue,
            };

            // Check if file needs re-indexing
            let status = cache.check_file(entry_path);
            if status == FileStatus::Unchanged {
                skipped_files += 1;
                continue;
            }

            let content = match fs::read_to_string(entry_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let line_count = content.lines().count();
            let relative_path = entry_path
                .strip_prefix(path)
                .unwrap_or(entry_path)
                .to_string_lossy();

            writer.add_document(doc!(
                file_path_field => relative_path.to_string(),
                content_field => content,
                language_field => language,
                line_count_field => line_count as u64
            ))?;

            // Update cache with new timestamp
            cache.update_file(entry_path);
            changed_files += 1;
            stats.files_indexed += 1;
            stats.total_lines += line_count;
        }

        writer.commit()?;
        cache.save()?;
        stats.duration_secs = start.elapsed().as_secs_f64();

        tracing::info!(
            "Incremental index: {} changed, {} unchanged",
            changed_files,
            skipped_files
        );

        Ok(stats)
    }

    /// Search the index for matching documents
    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let reader = self
            .reader
            .as_ref()
            .context("Index not opened for reading")?;
        let searcher = reader.searcher();

        let file_path_field = self.schema.get_field("file_path").unwrap();
        let content_field = self.schema.get_field("content").unwrap();
        let language_field = self.schema.get_field("language").unwrap();

        // Create query parser for content field
        let query_parser =
            QueryParser::for_index(&self.index, vec![content_field, file_path_field]);
        let query = query_parser.parse_query(query_str)?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;

            let file_path = retrieved_doc
                .get_first(file_path_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let content = retrieved_doc
                .get_first(content_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let language = retrieved_doc
                .get_first(language_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Find matching lines
            let matching_lines = find_matching_lines(&content, query_str);

            results.push(SearchResult {
                file_path,
                language,
                score,
                content,
                matching_lines,
            });
        }

        Ok(results)
    }
}

/// Find lines in content that match the query terms
fn find_matching_lines(content: &str, query: &str) -> Vec<(usize, String)> {
    let query_lower = query.to_lowercase();
    let terms: Vec<&str> = query_lower.split_whitespace().collect();

    content
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let line_lower = line.to_lowercase();
            terms.iter().any(|term| line_lower.contains(term))
        })
        .map(|(i, line)| (i + 1, line.to_string())) // 1-indexed lines
        .take(10) // Limit matches per file
        .collect()
}
