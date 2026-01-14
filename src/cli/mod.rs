//! CLI definitions using clap derive macros
//!
//! Provides a beautiful command-line interface with subcommands for:
//! - index: Build or rebuild the search index
//! - search: Query the index
//! - watch: Monitor filesystem for changes
//! - similar: Find semantically similar code
//! - config: Manage settings
//! - status: Show index health

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Seekr - Ultra-fast local hybrid semantic code search
#[derive(Parser, Debug)]
#[command(name = "seekr")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Index or reindex a directory for searching
    Index {
        /// Path to index (defaults to current directory)
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Force full reindex, ignoring cache
        #[arg(short, long, default_value = "false")]
        force: bool,

        /// Also build semantic embeddings index (slower but enables natural language search)
        #[arg(short, long, default_value = "false")]
        semantic: bool,
    },

    /// Search the indexed codebase
    Search {
        /// Search query (keywords or natural language)
        #[arg(value_name = "QUERY")]
        query: String,

        /// Maximum number of results to show
        #[arg(short, long, default_value = "10")]
        limit: usize,

        /// Lines of context to show around matches
        #[arg(short, long, default_value = "3")]
        context: usize,

        /// Use semantic search (embeddings) instead of lexical (BM25)
        #[arg(long, default_value = "false")]
        semantic: bool,

        /// Use hybrid search (combines BM25 + semantic with RRF fusion)
        #[arg(long, default_value = "false")]
        hybrid: bool,

        /// Alpha weight for hybrid search (0.0 = all semantic, 1.0 = all BM25)
        #[arg(long, default_value = "0.5")]
        alpha: f32,

        /// Output results as JSON (for tool integration)
        #[arg(long, default_value = "false")]
        json: bool,
    },

    /// Watch for file changes and auto-reindex
    Watch,

    /// Find code similar to a given snippet
    Similar {
        /// Path to the file containing the code
        #[arg(short, long)]
        file: PathBuf,

        /// Line range (e.g., "10..50")
        #[arg(short, long)]
        range: Option<String>,
    },

    /// Get or set configuration values
    Config {
        /// Configuration key (e.g., "alpha", "model")
        key: String,

        /// Value to set (omit to get current value)
        value: Option<String>,
    },

    /// Show index statistics and health
    Status,
}
