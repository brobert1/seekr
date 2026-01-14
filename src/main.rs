//! Seekr - Ultra-fast local hybrid semantic code search
//!
//! Combines BM25 lexical search (Tantivy) with semantic vector search
//! for the best of both worlds: exact matches when needed, conceptual
//! understanding when you need it.

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod cli;
mod indexer;
mod output;

use cli::{Cli, Commands};
use indexer::Indexer;
use output::ResultPrinter;

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Index { path, force } => {
            let path = path.unwrap_or_else(|| std::env::current_dir().unwrap());
            tracing::info!("Indexing: {:?} (force={})", path, force);
            
            let mut indexer = Indexer::new(&path, force)?;
            let stats = indexer.index_directory(&path)?;
            
            println!("\n✨ Indexing complete!");
            println!("   Files indexed: {}", stats.files_indexed);
            println!("   Total lines: {}", stats.total_lines);
            println!("   Time: {:.2}s", stats.duration_secs);
        }
        Commands::Search { query, limit, context } => {
            tracing::info!("Searching for: {}", query);
            
            let index_path = Indexer::default_index_path()?;
            let indexer = Indexer::open(&index_path)?;
            let results = indexer.search(&query, limit)?;
            
            let printer = ResultPrinter::new(context);
            printer.print_results(&results)?;
        }
        Commands::Watch => {
            tracing::info!("Starting file watcher...");
            println!("🔍 Watching for file changes... (Ctrl+C to stop)");
            // TODO: Implement filesystem watcher (Phase 3)
            println!("⚠️  Watch command not yet implemented");
        }
        Commands::Similar { file, range } => {
            tracing::info!("Finding similar code to {:?} range {:?}", file, range);
            // TODO: Implement semantic similarity (Phase 2/3)
            println!("⚠️  Similar command not yet implemented");
        }
        Commands::Config { key, value } => {
            if let Some(val) = value {
                tracing::info!("Setting config: {} = {}", key, val);
                // TODO: Implement config management
                println!("⚠️  Config command not yet implemented");
            } else {
                tracing::info!("Getting config: {}", key);
                println!("⚠️  Config command not yet implemented");
            }
        }
        Commands::Status => {
            let index_path = Indexer::default_index_path()?;
            match Indexer::get_status(&index_path) {
                Ok(status) => {
                    println!("\n📊 Index Status");
                    println!("   Path: {:?}", index_path);
                    println!("   Documents: {}", status.num_docs);
                    println!("   Size: {:.2} MB", status.size_bytes as f64 / 1_048_576.0);
                    println!("   Healthy: {}", if status.healthy { "✅" } else { "❌" });
                }
                Err(_) => {
                    println!("\n❌ No index found. Run `seekr index` first.");
                }
            }
        }
    }

    Ok(())
}
