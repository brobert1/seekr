//! Filesystem watcher for automatic re-indexing
//!
//! Uses the `notify` crate to watch for file changes and trigger
//! incremental index updates. Debounces rapid changes to avoid
//! overwhelming the indexer.

use anyhow::Result;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use crate::indexer::Indexer;

/// File system watcher that triggers re-indexing on changes
pub struct FileWatcher {
    debounce_ms: u64,
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self { debounce_ms: 500 }
    }
}

impl FileWatcher {

    /// Watch a directory and re-index on changes
    pub fn watch(&self, path: &Path) -> Result<()> {
        let (tx, rx) = mpsc::channel();

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            Config::default(),
        )?;

        watcher.watch(path, RecursiveMode::Recursive)?;

        println!(
            "🔍 Watching {} for changes... (Ctrl+C to stop)",
            path.display()
        );
        println!("   Debounce: {}ms\n", self.debounce_ms);

        let mut pending_files: HashSet<String> = HashSet::new();
        let mut last_index_time = std::time::Instant::now();
        let debounce_duration = Duration::from_millis(self.debounce_ms);

        loop {
            // Collect events with timeout
            match rx.recv_timeout(debounce_duration) {
                Ok(event) => {
                    // Filter for relevant events
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                            for path in event.paths {
                                // Skip non-code files and hidden directories
                                if Self::is_indexable(&path) {
                                    pending_files.insert(path.display().to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Debounce timeout - process pending changes
                    if !pending_files.is_empty() && last_index_time.elapsed() >= debounce_duration {
                        let count = pending_files.len();
                        println!("📝 {} file(s) changed, re-indexing...", count);

                        // Re-index
                        match self.reindex(path) {
                            Ok(stats) => {
                                println!(
                                    "   ✨ Indexed {} files in {:.2}s\n",
                                    stats.files_indexed, stats.duration_secs
                                );
                            }
                            Err(e) => {
                                println!("   ❌ Index error: {}\n", e);
                            }
                        }

                        pending_files.clear();
                        last_index_time = std::time::Instant::now();
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }

        Ok(())
    }

    /// Check if a file should be indexed
    fn is_indexable(path: &Path) -> bool {
        // Skip hidden files and directories
        if path.components().any(|c| {
            c.as_os_str()
                .to_str()
                .map(|s| s.starts_with('.'))
                .unwrap_or(false)
        }) {
            return false;
        }

        // Check extension
        match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => matches!(
                ext,
                "rs" | "py"
                    | "js"
                    | "jsx"
                    | "ts"
                    | "tsx"
                    | "go"
                    | "java"
                    | "c"
                    | "h"
                    | "cpp"
                    | "hpp"
                    | "rb"
                    | "md"
                    | "toml"
                    | "yaml"
                    | "json"
            ),
            None => false,
        }
    }

    /// Perform incremental re-indexing
    fn reindex(&self, path: &Path) -> Result<crate::indexer::IndexStats> {
        // Load file cache
        let home = dirs::home_dir().expect("Could not find home directory");
        let cache_path = home.join(".seekr");
        let mut file_cache = crate::cache::FileCache::load(&cache_path)?;

        let mut indexer = Indexer::new(path, false)?;
        indexer.index_directory_incremental(path, &mut file_cache)
    }
}
