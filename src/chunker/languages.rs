//! Language detection and tree-sitter language bindings
//!
//! Maps file extensions to languages and provides tree-sitter parsers

use std::path::Path;
use tree_sitter::Language as TSLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
    Unknown,
}

impl Language {
    /// Detect language from file path extension
    pub fn from_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => Language::Rust,
            Some("py") => Language::Python,
            Some("js" | "jsx" | "mjs" | "cjs") => Language::JavaScript,
            Some("ts" | "tsx") => Language::TypeScript,
            Some("go") => Language::Go,
            _ => Language::Unknown,
        }
    }

    /// Get the tree-sitter Language object for parsing
    pub fn tree_sitter_language(&self) -> Option<TSLanguage> {
        match self {
            Language::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
            Language::Python => Some(tree_sitter_python::LANGUAGE.into()),
            Language::JavaScript => Some(tree_sitter_javascript::LANGUAGE.into()),
            Language::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            Language::Go => Some(tree_sitter_go::LANGUAGE.into()),
            Language::Unknown => None,
        }
    }

    /// Get language name as string for embedding context
    pub fn name(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Go => "go",
            Language::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
