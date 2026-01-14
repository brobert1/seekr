//! Tantivy schema definition for the code search index
//!
//! Fields:
//! - file_path: Stored + indexed (for path-based search)
//! - content: Indexed + stored (main search target)
//! - language: Stored + fast (for filtering)
//! - line_count: Stored (for stats)

use tantivy::schema::*;

/// A search result from the index
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub file_path: String,
    pub language: String,
    pub score: f32,
    pub content: String,
    pub matching_lines: Vec<(usize, String)>, // (line_number, line_content)
}

/// Build the Tantivy schema for code indexing
pub fn build_schema() -> Schema {
    let mut schema_builder = Schema::builder();

    // File path - stored and indexed for path-based search
    schema_builder.add_text_field(
        "file_path",
        TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("default")
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored(),
    );

    // Content - main search field
    // Using default tokenizer which handles code reasonably well
    schema_builder.add_text_field(
        "content",
        TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("default")
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored(),
    );

    // Language - stored and fast field for filtering
    schema_builder.add_text_field(
        "language",
        TextOptions::default().set_stored(),
    );

    // Line count - stored for statistics
    schema_builder.add_u64_field("line_count", STORED);

    schema_builder.build()
}
