//! File modification cache for incremental indexing
//!
//! Tracks file modification timestamps to determine which files
//! need to be re-indexed. Stores timestamps in a JSON file.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Cache of file modification times
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FileCache {
    /// Map of file path -> last modified timestamp (as seconds since epoch)
    files: HashMap<String, u64>,
    /// Path to the cache file
    #[serde(skip)]
    cache_path: PathBuf,
}

/// Result of checking a file against the cache
#[derive(Debug, PartialEq)]
pub enum FileStatus {
    New,
    Modified,
    Unchanged,
}

impl FileCache {
    /// Load cache from disk or create new
    pub fn load(cache_dir: &Path) -> Result<Self> {
        let cache_path = cache_dir.join("file_cache.json");

        if cache_path.exists() {
            let content = fs::read_to_string(&cache_path)
                .context("Failed to read file cache")?;
            let mut cache: FileCache = serde_json::from_str(&content)
                .context("Failed to parse file cache")?;
            cache.cache_path = cache_path;
            Ok(cache)
        } else {
            Ok(Self {
                files: HashMap::new(),
                cache_path,
            })
        }
    }

    /// Check if a file needs to be re-indexed
    pub fn check_file(&self, path: &Path) -> FileStatus {
        let path_str = path.to_string_lossy().to_string();

        let current_mtime = match fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
        {
            Some(t) => t,
            None => return FileStatus::New,
        };

        match self.files.get(&path_str) {
            None => FileStatus::New,
            Some(&cached_mtime) => {
                if current_mtime > cached_mtime {
                    FileStatus::Modified
                } else {
                    FileStatus::Unchanged
                }
            }
        }
    }

    /// Update cache with file's current modification time
    pub fn update_file(&mut self, path: &Path) {
        let path_str = path.to_string_lossy().to_string();

        if let Ok(metadata) = fs::metadata(path) {
            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                    self.files.insert(path_str, duration.as_secs());
                }
            }
        }
    }

    /// Clear the entire cache
    pub fn clear(&mut self) {
        self.files.clear();
    }

    /// Save cache to disk
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&self)?;
        fs::write(&self.cache_path, content)?;
        Ok(())
    }
}
