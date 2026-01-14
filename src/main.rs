//! Seekr - Ultra-fast local hybrid semantic code search
//!
//! Combines BM25 lexical search (Tantivy) with semantic vector search
//! for the best of both worlds: exact matches when needed, conceptual
//! understanding when you need it.

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod cli;
mod chunker;
mod embedder;
mod indexer;
mod output;
mod semantic;
mod vector_store;

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
        Commands::Index { path, force, semantic } => {
            let path = path.unwrap_or_else(|| std::env::current_dir().unwrap());
            tracing::info!("Indexing: {:?} (force={}, semantic={})", path, force, semantic);
            
            // BM25 lexical index (always)
            let mut indexer = Indexer::new(&path, force)?;
            let stats = indexer.index_directory(&path)?;
            
            println!("\n✨ Lexical indexing complete!");
            println!("   Files indexed: {}", stats.files_indexed);
            println!("   Total lines: {}", stats.total_lines);
            println!("   Time: {:.2}s", stats.duration_secs);

            // Semantic index (if requested)
            if semantic {
                println!("\n🧠 Building semantic index (this may take a while on first run)...");
                
                let home = dirs::home_dir().expect("Could not find home directory");
                let semantic_path = home.join(".seekr");
                let mut semantic_indexer = semantic::SemanticIndexer::new(&semantic_path)?;
                
                // Collect files for semantic indexing
                let mut files: Vec<(std::path::PathBuf, String)> = Vec::new();
                let walker = ignore::WalkBuilder::new(&path)
                    .hidden(true)
                    .git_ignore(true)
                    .build();

                for entry in walker.filter_map(|e| e.ok()) {
                    let entry_path = entry.path();
                    if entry_path.is_file() {
                        if let Some(ext) = entry_path.extension().and_then(|e| e.to_str()) {
                            if matches!(ext, "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "go") {
                                if let Ok(content) = std::fs::read_to_string(entry_path) {
                                    files.push((entry_path.to_path_buf(), content));
                                }
                            }
                        }
                    }
                }

                let file_refs: Vec<(&std::path::Path, String)> = files
                    .iter()
                    .map(|(p, c)| (p.as_path(), c.clone()))
                    .collect();
                    
                let sem_stats = semantic_indexer.index_files(&file_refs)?;
                
                println!("   Chunks created: {}", sem_stats.chunks_created);
                println!("   Embeddings: {}", sem_stats.embeddings_generated);
                println!("   Time: {:.2}s", sem_stats.duration_secs);
            }
        }
        Commands::Search { query, limit, context, semantic } => {
            tracing::info!("Searching for: {} (semantic={})", query, semantic);
            
            if semantic {
                // Semantic search
                let home = dirs::home_dir().expect("Could not find home directory");
                let semantic_path = home.join(".seekr");
                let mut semantic_indexer = semantic::SemanticIndexer::new(&semantic_path)?;
                
                if !semantic_indexer.index_exists() {
                    println!("\n❌ No semantic index found. Run `seekr index --semantic` first.");
                    return Ok(());
                }
                
                let results = semantic_indexer.search(&query, limit)?;
                
                if results.is_empty() {
                    println!("\n{}", "No results found.".yellow());
                } else {
                    println!("\n{} {} results:\n", 
                        "Found".green(),
                        results.len()
                    );
                    
                    for (i, result) in results.iter().enumerate() {
                        println!(
                            "{} {} {} {}",
                            format!("[{}]", i + 1).cyan().bold(),
                            result.file_path.blue().bold(),
                            "·".dimmed(),
                            format!("similarity: {:.2}", result.similarity_score).dimmed()
                        );
                        println!("    {} {} {} {}",
                            "type:".dimmed(),
                            result.chunk_type.magenta(),
                            "lines:".dimmed(),
                            format!("{}-{}", result.start_line, result.end_line)
                        );
                        if let Some(name) = &result.name {
                            println!("    {} {}", "name:".dimmed(), name);
                        }
                        println!("    {}", result.content_preview.dimmed());
                        println!();
                    }
                }
            } else {
                // BM25 lexical search
                let index_path = Indexer::default_index_path()?;
                let indexer = Indexer::open(&index_path)?;
                let results = indexer.search(&query, limit)?;
                
                let printer = ResultPrinter::new(context);
                printer.print_results(&results)?;
            }
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
