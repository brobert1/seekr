<p align="center">
  <h1 align="center">🔍 Seekr</h1>
  <p align="center">
    <strong>Ultra-fast local hybrid semantic code search</strong>
  </p>
  <p align="center">
    Combines BM25 lexical precision with neural embeddings for intelligent, privacy-first code discovery.
  </p>
</p>

<p align="center">
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.75%2B-orange.svg" alt="Rust"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License"></a>
  <a href="#"><img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey.svg" alt="Platform"></a>
</p>

---

## Why Seekr?

Traditional code search tools force you to choose: **exact keyword matching** (fast but misses concepts) or **semantic search** (understands intent but slow on identifiers). Seekr combines both approaches, giving you:

- 🎯 **Exact matches** when you search for `getUserById`
- 🧠 **Conceptual matches** when you search for "authentication flow"
- ⚡ **Sub-20ms responses** on codebases with 100k+ lines

All processing happens locally. No cloud APIs, no data leaving your machine.

---

## Features

| Feature                    | Description                                                      |
| -------------------------- | ---------------------------------------------------------------- |
| **Hybrid Search**          | Fuses BM25 + semantic vectors using Reciprocal Rank Fusion (RRF) |
| **Semantic Understanding** | Finds code by concept using BGE neural embeddings                |
| **Incremental Indexing**   | Only re-indexes files that changed since last run                |
| **Watch Mode**             | Automatically updates index when files are saved                 |
| **Syntax Highlighting**    | Beautiful colorized output with context                          |
| **JSON Output**            | Machine-readable format for editor integration                   |
| **Privacy-First**          | 100% local — no telemetry, no cloud dependencies                 |

---

## Quick Start

```bash
# Install from source
git clone https://github.com/brobert1/seekr
cd seekr
cargo install --path .

# Initialize your project (builds lexical + semantic index)
cd /path/to/your/project
seekr init

# Search!
seekr search "error handling" --hybrid
```

---

## Usage

### Initialization

```bash
seekr init                    # Index current directory
seekr init /path/to/project   # Index specific path
```

First run downloads a 23MB embedding model. Subsequent runs are instant.

### Search Modes

```bash
# BM25 Lexical Search — fast, exact keyword matching
seekr search "useState"

# Semantic Search — understands concepts and synonyms
seekr search "user authentication" --semantic

# Hybrid Search — best of both (recommended)
seekr search "handle database errors" --hybrid
```

### Search Options

```bash
seekr search <QUERY> [OPTIONS]

Options:
  -l, --limit <N>      Maximum results to return [default: 10]
  -c, --context <N>    Lines of context around matches [default: 3]
      --semantic       Use semantic (embedding) search
      --hybrid         Use hybrid BM25 + semantic search
      --alpha <FLOAT>  Weight for BM25 in hybrid mode [default: 0.5]
      --json           Output results as JSON
```

### Watch Mode

```bash
seekr watch              # Monitor current directory
seekr watch /path/to/src # Monitor specific path
```

Watches for file changes with 500ms debouncing, then incrementally updates the index.

### Index Management

```bash
seekr index .           # Incremental update (only changed files)
seekr index . --force   # Full reindex from scratch
seekr status            # Show index health and statistics
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                           Source Files                               │
└─────────────────────────────────────────────────────────────────────┘
                                   │
                    ┌──────────────┴──────────────┐
                    ▼                              ▼
        ┌───────────────────┐          ┌───────────────────┐
        │   File Walker     │          │   Tree-sitter     │
        │   (respects       │          │   Chunker         │
        │   .gitignore)     │          │   (AST parsing)   │
        └───────────────────┘          └───────────────────┘
                    │                              │
                    ▼                              ▼
        ┌───────────────────┐          ┌───────────────────┐
        │     Tantivy       │          │   BGE-small-en    │
        │   BM25 Index      │          │   Embeddings      │
        └───────────────────┘          └───────────────────┘
                    │                              │
                    ▼                              ▼
        ┌───────────────────┐          ┌───────────────────┐
        │  Lexical Results  │          │  USearch HNSW     │
        │                   │          │  Vector Index     │
        └───────────────────┘          └───────────────────┘
                    │                              │
                    └──────────────┬───────────────┘
                                   ▼
                    ┌───────────────────────────┐
                    │   Reciprocal Rank Fusion  │
                    │   (Hybrid Ranking)        │
                    └───────────────────────────┘
                                   │
                                   ▼
                    ┌───────────────────────────┐
                    │   Syntax-Highlighted      │
                    │   Results                 │
                    └───────────────────────────┘
```

### Technology Stack

| Component      | Technology                                         | Purpose                            |
| -------------- | -------------------------------------------------- | ---------------------------------- |
| Lexical Search | [Tantivy](https://github.com/quickwit-oss/tantivy) | BM25 full-text indexing            |
| Code Parsing   | [Tree-sitter](https://tree-sitter.github.io/)      | AST-based semantic chunking        |
| Embeddings     | [Fastembed](https://github.com/qdrant/fastembed)   | Local BGE-small-en-v1.5 (384d)     |
| Vector Search  | [USearch](https://github.com/unum-cloud/usearch)   | HNSW approximate nearest neighbors |
| File Watching  | [Notify](https://github.com/notify-rs/notify)      | Cross-platform filesystem events   |
| CLI            | [Clap](https://github.com/clap-rs/clap)            | Argument parsing and help          |

---

## Supported Languages

| Language   | Extensions                        | Semantic Chunking |
| ---------- | --------------------------------- | ----------------- |
| Rust       | `.rs`                             | ✅ Tree-sitter    |
| Python     | `.py`                             | ✅ Tree-sitter    |
| TypeScript | `.ts`, `.tsx`                     | ✅ Tree-sitter    |
| JavaScript | `.js`, `.jsx`                     | ✅ Tree-sitter    |
| Go         | `.go`                             | ✅ Tree-sitter    |
| Java       | `.java`                           | Sliding window    |
| C/C++      | `.c`, `.h`, `.cpp`, `.hpp`, `.cc` | Sliding window    |
| Ruby       | `.rb`                             | Sliding window    |
| Markdown   | `.md`                             | Sliding window    |
| Config     | `.toml`, `.yaml`, `.yml`, `.json` | Sliding window    |

Languages with Tree-sitter support get intelligent chunking by functions/classes. Others use overlapping sliding windows.

---

## Performance

Benchmarked on a 116k LOC TypeScript/JavaScript project:

| Metric          | Value   |
| --------------- | ------- |
| BM25 Index Time | 0.19s   |
| Query Latency   | 5-20ms  |
| Index Size      | 2.33 MB |
| Files Indexed   | 1,606   |

> Automatically skipped 55,000+ files in `node_modules` via `.gitignore` integration.

---

## Configuration

All data is stored in `~/.seekr/`:

| Path                       | Description                    |
| -------------------------- | ------------------------------ |
| `~/.seekr/index/`          | Tantivy BM25 index             |
| `~/.seekr/semantic/`       | Vector embeddings and metadata |
| `~/.seekr/file_cache.json` | File modification timestamps   |
| `~/.seekr/workspace.txt`   | Indexed workspace path         |

### Reset Index

```bash
rm -rf ~/.seekr
seekr init
```

---

## Installation

### From Source

```bash
git clone https://github.com/brobert1/seekr
cd seekr
cargo install --path .
```

### Requirements

- **Rust** 1.75 or later
- **Platform:** macOS, Linux (Windows untested)
- **Disk:** ~150MB for embedding model (first run only)

---

## Contributing

Contributions are welcome. The codebase compiles with zero warnings.

```bash
cargo build            # Development build
cargo test             # Run test suite
cargo clippy           # Lint
cargo build --release  # Optimized build
```

---

## License

MIT © Robert Bercaru
