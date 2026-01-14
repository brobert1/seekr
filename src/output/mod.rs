//! Pretty terminal output with syntax highlighting
//!
//! Uses syntect for code highlighting and colored for terminal colors.
//! Inspired by bat's beautiful output style.

use anyhow::Result;
use colored::*;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;

use crate::indexer::SearchResult;

/// Handles formatting and printing search results
pub struct ResultPrinter {
    context_lines: usize,
    ps: SyntaxSet,
    ts: ThemeSet,
}

impl ResultPrinter {
    pub fn new(context_lines: usize) -> Self {
        Self {
            context_lines,
            ps: SyntaxSet::load_defaults_newlines(),
            ts: ThemeSet::load_defaults(),
        }
    }

    /// Print search results with syntax highlighting
    pub fn print_results(&self, results: &[SearchResult]) -> Result<()> {
        if results.is_empty() {
            println!("\n{}", "No results found.".yellow());
            return Ok(());
        }

        println!(
            "\n{} {}",
            "Found".green().bold(),
            format!("{} results:", results.len()).green()
        );
        println!();

        for (i, result) in results.iter().enumerate() {
            self.print_result(i + 1, result)?;
        }

        Ok(())
    }

    fn print_result(&self, index: usize, result: &SearchResult) -> Result<()> {
        // Header with file path and score
        println!(
            "{} {} {} {}",
            format!("[{}]", index).cyan().bold(),
            result.file_path.blue().bold(),
            "·".dimmed(),
            format!("score: {:.2}", result.score).dimmed()
        );

        // Language badge
        println!(
            "    {} {}",
            "language:".dimmed(),
            result.language.magenta()
        );

        // Get syntax for highlighting
        let syntax = self
            .ps
            .find_syntax_by_extension(&result.language)
            .or_else(|| self.ps.find_syntax_by_extension("txt"))
            .unwrap_or_else(|| self.ps.find_syntax_plain_text());

        let theme = &self.ts.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        // Print matching lines with context
        let lines: Vec<&str> = result.content.lines().collect();

        for (line_num, _line_content) in &result.matching_lines {
            let start = line_num.saturating_sub(self.context_lines + 1);
            let end = (*line_num + self.context_lines).min(lines.len());

            println!();
            println!("    {}", "─".repeat(60).dimmed());

            for i in start..end {
                let line = lines.get(i).unwrap_or(&"");
                let line_number = i + 1;

                // Highlight the match line differently
                let prefix = if line_number == *line_num {
                    format!("{:>4} │ ", line_number).yellow().bold()
                } else {
                    format!("{:>4} │ ", line_number).dimmed()
                };

                // Syntax highlight the code
                if let Ok(ranges) = highlighter.highlight_line(line, &self.ps) {
                    let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
                    println!("    {}{}\x1b[0m", prefix, escaped);
                } else {
                    println!("    {}{}", prefix, line);
                }
            }
        }

        println!();
        Ok(())
    }
}
