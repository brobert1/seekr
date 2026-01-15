//! Seekr - Ultra-fast local hybrid semantic code search
//!
//! Combines BM25 lexical search (Tantivy) with semantic vector search
//! for the best of both worlds: exact matches when needed, conceptual
//! understanding when you need it.

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod cache;
mod chunker;
mod cli;
mod embedder;
mod indexer;
mod output;
mod ranker;
mod semantic;
mod vector_store;
mod watcher;

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
        Commands::Index {
            path,
            force,
            semantic,
        } => {
            let path = path.unwrap_or_else(|| std::env::current_dir().unwrap());
            tracing::info!(
                "Indexing: {:?} (force={}, semantic={})",
                path,
                force,
                semantic
            );

            // Load file cache for incremental indexing
            let home = dirs::home_dir().expect("Could not find home directory");
            let cache_path = home.join(".seekr");
            let mut file_cache = cache::FileCache::load(&cache_path)?;

            // BM25 lexical index
            let mut indexer = Indexer::new(&path, force)?;
            let stats = if force {
                // Force = full reindex
                file_cache.clear();
                let s = indexer.index_directory(&path)?;
                // Update cache for all files
                for entry in ignore::WalkBuilder::new(&path)
                    .hidden(true)
                    .git_ignore(true)
                    .build()
                    .filter_map(|e| e.ok())
                {
                    if entry.path().is_file() {
                        file_cache.update_file(entry.path());
                    }
                }
                file_cache.save()?;
                s
            } else {
                // Incremental = only changed files
                indexer.index_directory_incremental(&path, &mut file_cache)?
            };

            if force {
                println!("\n✨ Lexical indexing complete! (full reindex)");
            } else {
                println!("\n✨ Lexical indexing complete! (incremental)");
            }
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
        Commands::Search {
            query,
            limit,
            context,
            semantic,
            hybrid,
            alpha,
            json,
        } => {
            tracing::info!(
                "Searching for: {} (semantic={}, hybrid={}, alpha={}, json={})",
                query,
                semantic,
                hybrid,
                alpha,
                json
            );

            if hybrid {
                // Hybrid search: combine BM25 + semantic
                if !json {
                    println!("\n🔀 Hybrid search (α={:.2})...", alpha);
                }

                // Get BM25 results
                let index_path = Indexer::default_index_path()?;
                let indexer = Indexer::open(&index_path)?;
                let bm25_results = indexer.search(&query, limit * 2)?;

                // Get semantic results
                let home = dirs::home_dir().expect("Could not find home directory");
                let semantic_path = home.join(".seekr");
                let mut semantic_indexer = semantic::SemanticIndexer::new(&semantic_path)?;

                if !semantic_indexer.index_exists() {
                    println!(
                        "\n⚠️  No semantic index. Run `seekr index --semantic` for best results."
                    );
                    println!("   Falling back to lexical search only.\n");
                    let printer = ResultPrinter::new(context);
                    printer.print_results(&bm25_results)?;
                    return Ok(());
                }

                let sem_results = semantic_indexer.search(&query, limit * 2)?;

                // Convert to RankedResults
                let lexical: Vec<ranker::RankedResult> = bm25_results
                    .iter()
                    .map(|r| ranker::RankedResult {
                        file_path: r.file_path.clone(),
                        score: r.score,
                        source: ranker::SearchSource::Lexical,
                        start_line: r.matching_lines.first().map(|(l, _)| *l).unwrap_or(1),
                        end_line: r.matching_lines.last().map(|(l, _)| *l).unwrap_or(1),
                        content_preview: r
                            .matching_lines
                            .first()
                            .map(|(_, c)| c.clone())
                            .unwrap_or_default(),
                        name: None,
                    })
                    .collect();

                let semantic_ranked: Vec<ranker::RankedResult> = sem_results
                    .iter()
                    .map(|r| ranker::RankedResult {
                        file_path: r.file_path.clone(),
                        score: r.similarity_score,
                        source: ranker::SearchSource::Semantic,
                        start_line: r.start_line,
                        end_line: r.end_line,
                        content_preview: r.content_preview.clone(),
                        name: r.name.clone(),
                    })
                    .collect();

                // Fuse results
                let ranker_config = ranker::HybridConfig {
                    alpha,
                    rrf_k: 60.0,
                    use_rrf: true,
                };
                let hybrid_ranker = ranker::HybridRanker::new(ranker_config);
                let fused = hybrid_ranker.fuse(lexical, semantic_ranked, limit);

                // Print fused results
                if json {
                    // JSON output for tool integration
                    let json_results: Vec<serde_json::Value> = fused
                        .iter()
                        .map(|r| {
                            serde_json::json!({
                                "file": r.file_path,
                                "score": r.score,
                                "start_line": r.start_line,
                                "end_line": r.end_line,
                                "name": r.name,
                                "preview": r.content_preview,
                                "source": format!("{:?}", r.source)
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&json_results)?);
                } else if fused.is_empty() {
                    println!("\n{}", "No results found.".yellow());
                } else {
                    println!("\n{} {} hybrid results:\n", "Found".green(), fused.len());

                    for (i, result) in fused.iter().enumerate() {
                        println!(
                            "{} {} {} {}",
                            format!("[{}]", i + 1).cyan().bold(),
                            result.file_path.blue().bold(),
                            "·".dimmed(),
                            format!("score: {:.3}", result.score).dimmed()
                        );
                        println!(
                            "    {} {} {}",
                            "lines:".dimmed(),
                            format!("{}-{}", result.start_line, result.end_line),
                            format!("[{:?}]", result.source).dimmed()
                        );
                        if let Some(name) = &result.name {
                            println!("    {} {}", "name:".dimmed(), name);
                        }
                        if !result.content_preview.is_empty() {
                            println!(
                                "    {}",
                                result
                                    .content_preview
                                    .chars()
                                    .take(100)
                                    .collect::<String>()
                                    .dimmed()
                            );
                        }
                        println!();
                    }
                }
            } else if semantic {
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
                    println!("\n{} {} results:\n", "Found".green(), results.len());

                    for (i, result) in results.iter().enumerate() {
                        println!(
                            "{} {} {} {}",
                            format!("[{}]", i + 1).cyan().bold(),
                            result.file_path.blue().bold(),
                            "·".dimmed(),
                            format!("similarity: {:.2}", result.similarity_score).dimmed()
                        );
                        println!(
                            "    {} {} {} {}",
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

                if json {
                    let json_results: Vec<serde_json::Value> = results
                        .iter()
                        .map(|r| {
                            serde_json::json!({
                                "file": r.file_path,
                                "score": r.score,
                                "language": r.language,
                                "matching_lines": r.matching_lines.iter().map(|(l, c)| {
                                    serde_json::json!({"line": l, "content": c})
                                }).collect::<Vec<_>>()
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&json_results)?);
                } else {
                    let printer = ResultPrinter::new(context);
                    printer.print_results(&results)?;
                }
            }
        }
        Commands::Watch => {
            tracing::info!("Starting file watcher...");
            let path = std::env::current_dir()?;
            let file_watcher = watcher::FileWatcher::default();
            file_watcher.watch(&path)?;
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
        Commands::Init { path } => {
            let path = path.unwrap_or_else(|| std::env::current_dir().unwrap());
            println!("\n🚀 Initializing seekr in {:?}...\n", path);

            // Step 1: Build BM25 index
            println!("📚 Step 1/2: Building lexical index...");
            let mut indexer = Indexer::new(&path, true)?;
            let stats = indexer.index_directory(&path)?;
            println!(
                "   ✅ Indexed {} files ({} lines) in {:.2}s\n",
                stats.files_indexed, stats.total_lines, stats.duration_secs
            );

            // Step 2: Build semantic index
            println!("🧠 Step 2/2: Building semantic index...");
            println!("   (This downloads a 23MB model on first run)\n");

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
            println!(
                "   ✅ Created {} chunks, {} embeddings in {:.2}s\n",
                sem_stats.chunks_created, sem_stats.embeddings_generated, sem_stats.duration_secs
            );

            // Summary
            println!("🎉 Initialization complete!\n");
            println!("Try these commands:");
            println!("   seekr search \"your query\"           # Keyword search");
            println!("   seekr search \"what you want\" --semantic  # Natural language");
            println!("   seekr search \"query\" --hybrid       # Best of both");
            println!("   seekr watch                         # Auto-reindex on changes");
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
