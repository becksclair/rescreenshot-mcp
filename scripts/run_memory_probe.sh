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
# - Valid restore token
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
cargo build --release --features linux-wayland --quiet
echo -e "${GREEN}✓ Build complete${NC}"
echo ""

# Memory profiling parameters
MASSIF_OUT="massif.out.$$"
MEMORY_THRESHOLD_MB=200

echo -e "${BLUE}Starting memory profiling...${NC}"
echo -e "${YELLOW}NOTE: This requires a test harness binary for sequential captures.${NC}"
echo -e "${YELLOW}Current implementation is a stub. Full implementation pending (Task 11).${NC}"
echo ""

echo -e "${YELLOW}=== Memory Profiling Status ===${NC}"
echo -e "${YELLOW}This script is a stub that shows the intended workflow.${NC}"
echo ""
echo -e "${BLUE}Intended workflow:${NC}"
echo "1. Run Valgrind Massif on capture operations"
echo "2. Perform ${NUM_CAPTURES} sequential captures"
echo "3. Analyze peak memory usage (target: <200MB)"
echo "4. Check for memory leaks (using valgrind --leak-check=full)"
echo ""
echo -e "${BLUE}Example Valgrind command:${NC}"
echo "  valgrind --tool=massif --massif-out-file=${MASSIF_OUT} \\"
echo "    ./target/release/screenshot-mcp [capture-command]"
echo ""
echo -e "${BLUE}To analyze results:${NC}"
echo "  ms_print ${MASSIF_OUT}"
echo "  grep \"Peak\" ${MASSIF_OUT}"
echo ""
echo -e "${BLUE}For manual memory validation:${NC}"
echo "  1. Run: /usr/bin/time -v ./target/release/screenshot-mcp [capture]"
echo "  2. Check 'Maximum resident set size' in output"
echo "  3. Verify < ${MEMORY_THRESHOLD_MB}MB (${MEMORY_THRESHOLD_MB}000 KB)"
echo ""
echo -e "${YELLOW}Full implementation requires:${NC}"
echo "  - Test harness binary that performs sequential captures"
echo "  - Integration with measure-capture tool (Task 11)"
echo "  - Automated parsing of Massif output"
