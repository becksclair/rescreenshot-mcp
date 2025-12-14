#!/usr/bin/env bash
# Memory Profiling Script
#
# Profiles memory usage of screenshot-mcp capture operations using Valgrind Massif.
# Validates M2 exit criteria:
# - Memory peak: <200MB during capture
# - No memory leaks after 10 sequential captures
#
# Prerequisites:
# - valgrind installed
# - Live Wayland session with portal
# - Valid restore token (run prime-consent first)
#
# Usage:
#   ./scripts/run_memory_probe.sh [NUM_CAPTURES]
#
# Example:
#   ./scripts/run_memory_probe.sh 10

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

NUM_CAPTURES="${1:-10}"
MEMORY_THRESHOLD_MB=200
SOURCE_ID="wayland-mem-$(date +%s)"

echo -e "${BLUE}=== Memory Profiling Script ===${NC}"
echo -e "${BLUE}Target: ${NUM_CAPTURES} sequential captures${NC}"
echo ""

# Check valgrind
if ! command -v valgrind &> /dev/null; then
    echo -e "${RED}ERROR: valgrind not installed${NC}"
    echo "Install with: sudo apt install valgrind"
    exit 1
fi

echo -e "${GREEN}✓ valgrind found${NC}"

# Check environment
if [ -z "${WAYLAND_DISPLAY:-}" ]; then
    echo -e "${RED}ERROR: WAYLAND_DISPLAY not set${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Wayland environment detected${NC}"
echo ""

# Build release binary
echo -e "${BLUE}Building release binary...${NC}"
cargo build --bin measure-capture --features perf-tests --release --quiet
echo -e "${GREEN}✓ Build complete${NC}"
echo ""

# Memory profiling parameters
RESULTS_DIR="./perf-results"
TIMESTAMP=$(date +"%Y%m%d-%H%M%S")
MASSIF_OUT="$RESULTS_DIR/massif-$TIMESTAMP.out"
LEAK_LOG="$RESULTS_DIR/leak-check-$TIMESTAMP.log"

mkdir -p "$RESULTS_DIR"

# ==============================================================================
# Step 1: Prime Consent (to get valid token)
# ==============================================================================

echo -e "${BLUE}Step 1: Priming consent...${NC}"
echo -e "${YELLOW}You will see a portal dialog. Grant permission to continue.${NC}"
echo ""

if ! ./target/release/measure-capture prime-consent "$SOURCE_ID" > /dev/null 2>&1; then
    echo -e "${RED}✗ Prime consent failed${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Prime consent successful${NC}"
echo ""
sleep 2

# ==============================================================================
# Step 2: Memory Peak Measurement with Massif
# ==============================================================================

echo -e "${BLUE}Step 2: Measuring memory peak with Valgrind Massif...${NC}"
echo -e "${YELLOW}Running ${NUM_CAPTURES} captures under Massif (this will be slow)...${NC}"
echo ""

valgrind --tool=massif \
    --massif-out-file="$MASSIF_OUT" \
    --pages-as-heap=yes \
    ./target/release/measure-capture headless-batch --captures "$NUM_CAPTURES" "$SOURCE_ID" > /dev/null 2>&1 || true

echo -e "${GREEN}✓ Massif profiling complete${NC}"
echo ""

# Parse peak memory from Massif output
if [ -f "$MASSIF_OUT" ]; then
    PEAK_BYTES=$(grep -oP 'mem_heap_B=\K\d+' "$MASSIF_OUT" | sort -n | tail -1)
    PEAK_MB=$((PEAK_BYTES / 1024 / 1024))

    echo -e "${BLUE}Memory Peak Analysis:${NC}"
    echo "  Peak Memory: ${PEAK_MB}MB"
    echo "  Threshold: <${MEMORY_THRESHOLD_MB}MB"

    if [ "$PEAK_MB" -lt "$MEMORY_THRESHOLD_MB" ]; then
        echo -e "  Result: ${GREEN}✓ PASS${NC}"
        MEMORY_PASSED=true
    else
        echo -e "  Result: ${RED}✗ FAIL (exceeded threshold by $((PEAK_MB - MEMORY_THRESHOLD_MB))MB)${NC}"
        MEMORY_PASSED=false
    fi
else
    echo -e "${YELLOW}WARNING: Massif output file not found${NC}"
    MEMORY_PASSED=false
fi

echo ""

# ==============================================================================
# Step 3: Memory Leak Check
# ==============================================================================

echo -e "${BLUE}Step 3: Checking for memory leaks...${NC}"
echo -e "${YELLOW}Running ${NUM_CAPTURES} captures with leak detection...${NC}"
echo ""

valgrind --leak-check=full \
    --show-leak-kinds=all \
    --log-file="$LEAK_LOG" \
    ./target/release/measure-capture headless-batch --captures "$NUM_CAPTURES" "$SOURCE_ID" > /dev/null 2>&1 || true

echo -e "${GREEN}✓ Leak check complete${NC}"
echo ""

# Parse leak results
if [ -f "$LEAK_LOG" ]; then
    DEFINITELY_LOST=$(grep -oP 'definitely lost: \K[\d,]+' "$LEAK_LOG" | tr -d ',' || echo "0")
    POSSIBLY_LOST=$(grep -oP 'possibly lost: \K[\d,]+' "$LEAK_LOG" | tr -d ',' || echo "0")

    echo -e "${BLUE}Memory Leak Analysis:${NC}"
    echo "  Definitely lost: ${DEFINITELY_LOST} bytes"
    echo "  Possibly lost: ${POSSIBLY_LOST} bytes"

    if [ "$DEFINITELY_LOST" -eq 0 ]; then
        echo -e "  Result: ${GREEN}✓ PASS (no definite leaks)${NC}"
        LEAK_PASSED=true
    else
        echo -e "  Result: ${YELLOW}⚠ WARNING (${DEFINITELY_LOST} bytes definitely lost)${NC}"
        LEAK_PASSED=false
    fi
else
    echo -e "${YELLOW}WARNING: Leak log not found${NC}"
    LEAK_PASSED=false
fi

echo ""

# ==============================================================================
# Summary
# ==============================================================================

echo -e "${BLUE}=== Memory Profiling Summary ===${NC}"
echo ""
echo "Results saved to: $RESULTS_DIR/"
echo "  - $MASSIF_OUT"
echo "  - $LEAK_LOG"
echo ""

echo "Profiling Results:"
if [ "$MEMORY_PASSED" = true ]; then
    echo -e "  ${GREEN}✓ Memory Peak <${MEMORY_THRESHOLD_MB}MB${NC}"
else
    echo -e "  ${RED}✗ Memory Peak exceeded${NC}"
fi

if [ "$LEAK_PASSED" = true ]; then
    echo -e "  ${GREEN}✓ No Memory Leaks${NC}"
else
    echo -e "  ${YELLOW}⚠ Possible Memory Leaks${NC}"
fi

echo ""
echo "View detailed results:"
echo "  ms_print $MASSIF_OUT"
echo "  cat $LEAK_LOG"
echo ""

# Exit code
if [ "$MEMORY_PASSED" = true ] && [ "$LEAK_PASSED" = true ]; then
    echo -e "${GREEN}=== MEMORY PROFILING PASSED ===${NC}"
    exit 0
else
    echo -e "${RED}=== MEMORY PROFILING FAILED ===${NC}"
    exit 1
fi
