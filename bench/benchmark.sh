#!/usr/bin/env zsh
# Seekr vs Ripgrep Benchmark Suite
# Compares query latency, accuracy, and features

set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Configuration
TARGET_DIR="${1:-$(pwd)}"
RESULTS_FILE="bench/results.md"

# Test queries
typeset -a QUERIES
QUERIES=(
    "function"
    "error"
    "async"
    "import"
    "TODO"
    "authentication"
    "handleSubmit"
    "useState"
    "export"
    "interface"
)

echo "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo "${BLUE}  SEEKR vs RIPGREP BENCHMARK${NC}"
echo "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "${YELLOW}Target:${NC} $TARGET_DIR"
echo ""

# Check dependencies
if ! command -v rg &> /dev/null; then
    echo "Error: ripgrep (rg) not found. Install with: brew install ripgrep"
    exit 1
fi

if ! command -v seekr &> /dev/null; then
    echo "Error: seekr not found. Install with: cargo install --path ."
    exit 1
fi

# Ensure seekr index exists
echo "${YELLOW}[1/4]${NC} Ensuring seekr index is up to date..."
seekr index "$TARGET_DIR" > /dev/null 2>&1 || seekr index "$TARGET_DIR" --force > /dev/null 2>&1
echo "      ✓ Index ready"
echo ""

# Initialize results file
mkdir -p bench
cat > "$RESULTS_FILE" << 'EOF'
# Benchmark Results: Seekr vs Ripgrep

EOF

echo "**Target:** \`$TARGET_DIR\`" >> "$RESULTS_FILE"
echo "**Date:** $(date '+%Y-%m-%d %H:%M')" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"
echo "---" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"

# Run benchmarks
echo "${YELLOW}[2/4]${NC} Running query latency benchmarks..."
echo ""

echo "## Query Latency (milliseconds)" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"
echo "| Query | Ripgrep | Seekr | Δ | Winner |" >> "$RESULTS_FILE"
echo "|-------|---------|-------|---|--------|" >> "$RESULTS_FILE"

RG_TOTAL=0
SEEKR_TOTAL=0
RG_WINS=0
SEEKR_WINS=0

for query in "${QUERIES[@]}"; do
    # Time ripgrep (warmup + measure)
    rg -l "$query" "$TARGET_DIR" > /dev/null 2>&1 || true
    RG_START=$(($(python3 -c 'import time; print(int(time.time()*1000))')))
    for i in {1..3}; do
        rg -l "$query" "$TARGET_DIR" > /dev/null 2>&1 || true
    done
    RG_END=$(($(python3 -c 'import time; print(int(time.time()*1000))')))
    RG_TIME=$(( (RG_END - RG_START) / 3 ))
    
    # Time seekr (warmup + measure)
    seekr search "$query" --limit 10 > /dev/null 2>&1 || true
    SEEKR_START=$(($(python3 -c 'import time; print(int(time.time()*1000))')))
    for i in {1..3}; do
        seekr search "$query" --limit 10 > /dev/null 2>&1 || true
    done
    SEEKR_END=$(($(python3 -c 'import time; print(int(time.time()*1000))')))
    SEEKR_TIME=$(( (SEEKR_END - SEEKR_START) / 3 ))
    
    RG_TOTAL=$((RG_TOTAL + RG_TIME))
    SEEKR_TOTAL=$((SEEKR_TOTAL + SEEKR_TIME))
    
    DIFF=$((SEEKR_TIME - RG_TIME))
    
    if [ "$RG_TIME" -lt "$SEEKR_TIME" ]; then
        WINNER="ripgrep"
        RG_WINS=$((RG_WINS + 1))
    elif [ "$SEEKR_TIME" -lt "$RG_TIME" ]; then
        WINNER="**seekr**"
        SEEKR_WINS=$((SEEKR_WINS + 1))
    else
        WINNER="tie"
    fi
    
    echo "| \`$query\` | $RG_TIME | $SEEKR_TIME | ${DIFF:+$DIFF} | $WINNER |" >> "$RESULTS_FILE"
    printf "  %-18s  rg: %4dms  seekr: %4dms  %s\n" "\"$query\"" "$RG_TIME" "$SEEKR_TIME" "→ $WINNER"
done

RG_AVG=$((RG_TOTAL / ${#QUERIES[@]}))
SEEKR_AVG=$((SEEKR_TOTAL / ${#QUERIES[@]}))

echo "" >> "$RESULTS_FILE"
echo "**Average:** Ripgrep ${RG_AVG}ms vs Seekr ${SEEKR_AVG}ms" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"
echo "**Wins:** Ripgrep $RG_WINS / Seekr $SEEKR_WINS" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"

echo ""
echo "  ${GREEN}Average:${NC} ripgrep ${RG_AVG}ms vs seekr ${SEEKR_AVG}ms"
echo "  ${GREEN}Wins:${NC}    ripgrep $RG_WINS vs seekr $SEEKR_WINS"
echo ""

# Result count comparison
echo "${YELLOW}[3/4]${NC} Comparing result counts..."
echo ""

echo "## Result Counts" >> "$RESULTS_FILE"
echo "" >> "$RESULTS_FILE"
echo "| Query | Ripgrep (files) | Seekr (matches) |" >> "$RESULTS_FILE"
echo "|-------|-----------------|-----------------|" >> "$RESULTS_FILE"
# Disable error exit for this section since grep returns 1 on no match
set +e
for query in "${QUERIES[@]}"; do
    RG_COUNT=$(rg -l "$query" "$TARGET_DIR" 2>/dev/null | wc -l | xargs)
    SEEKR_COUNT=$(seekr search "$query" --limit 100 2>/dev/null | grep -c "│" || true)
    
    echo "| \`$query\` | ${RG_COUNT:-0} | ${SEEKR_COUNT:-0} |" >> "$RESULTS_FILE"
    echo "  \"$query\"            rg: ${RG_COUNT:-0} files  seekr: ${SEEKR_COUNT:-0}"
done
set -e

# Feature comparison
echo ""
echo "${YELLOW}[4/4]${NC} Feature comparison..."
echo ""

cat >> "$RESULTS_FILE" << 'EOF'
## Feature Comparison

| Feature | Ripgrep | Seekr |
|---------|:-------:|:-----:|
| Keyword Search | ✅ | ✅ |
| Regex Pattern Matching | ✅ | ✅ |
| Semantic Search | ❌ | ✅ |
| Hybrid BM25+Vector | ❌ | ✅ |
| Pre-built Index | ❌ | ✅ |
| Incremental Updates | ❌ | ✅ |
| File Watch Mode | ❌ | ✅ |
| JSON Output | ✅ | ✅ |
| Syntax Highlighting | ❌ | ✅ |
| .gitignore Respect | ✅ | ✅ |
| Zero Setup | ✅ | ❌ |

---

## Analysis

**Ripgrep** excels at:
- Zero-setup grep across any directory
- Regex-heavy searches
- One-off searches in unfamiliar codebases

**Seekr** excels at:
- Conceptual/semantic code discovery
- Daily development in a known codebase
- Finding code by intent rather than exact keywords
- Integration with editors via JSON output

EOF

echo "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo "${GREEN}  BENCHMARK COMPLETE${NC}"
echo "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "Results saved to: ${BLUE}$RESULTS_FILE${NC}"
echo ""

# Summary
if [ "$RG_AVG" -lt "$SEEKR_AVG" ]; then
    DIFF_PCT=$(( (SEEKR_AVG - RG_AVG) * 100 / RG_AVG ))
    echo "${YELLOW}Summary:${NC} Ripgrep is ${DIFF_PCT}% faster on raw keyword queries."
    echo "         Seekr provides semantic search that ripgrep cannot do."
else
    echo "${YELLOW}Summary:${NC} Seekr matches ripgrep on indexed queries AND"
    echo "         provides semantic search capabilities!"
fi
echo ""
