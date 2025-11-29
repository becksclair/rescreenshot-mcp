#!/usr/bin/env bash
# Performance Test Suite Runner
#
# Measures screenshot-mcp performance metrics against M2 exit criteria:
# - Prime consent flow: <5s (excluding user interaction)
# - Headless capture latency: <2s (P95)
# - Token rotation overhead: <100ms
#
# Prerequisites:
# - Live Wayland session with portal
# - xdg-desktop-portal installed and running
#
# Usage:
#   ./scripts/run_performance_suite.sh

set -euo pipefail

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SOURCE_ID="wayland-perf-$(date +%s)"
RESULTS_DIR="./perf-results"
TIMESTAMP=$(date +"%Y%m%d-%H%M%S")

echo -e "${BLUE}=== screenshot-mcp Performance Suite ===${NC}"
echo ""

# Check environment
if [ -z "${WAYLAND_DISPLAY:-}" ]; then
    echo -e "${RED}ERROR: WAYLAND_DISPLAY not set${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Wayland environment detected${NC}"
echo ""

# Build measure-capture tool
echo -e "${BLUE}Building performance measurement tool...${NC}"
if cargo build --bin measure-capture --features perf-tests,linux-wayland --release --quiet; then
    echo -e "${GREEN}✓ Build successful${NC}"
else
    echo -e "${RED}✗ Build failed${NC}"
    exit 1
fi

echo ""

# Create results directory
mkdir -p "$RESULTS_DIR"

# ==============================================================================
# Test 1: Prime Consent
# ==============================================================================

echo -e "${BLUE}=== Test 1: Prime Consent Flow ===${NC}"
echo ""
echo -e "${YELLOW}IMPORTANT: You will see a portal permission dialog.${NC}"
echo -e "${YELLOW}Grant permission, then the test will measure the flow time.${NC}"
echo ""
read -p "Press Enter to start prime consent test..."

if ./target/release/measure-capture prime-consent "$SOURCE_ID" > "$RESULTS_DIR/prime-consent-$TIMESTAMP.json" 2>&1; then
    echo -e "${GREEN}✓ Prime consent test PASSED${NC}"
    PRIME_PASSED=true
else
    echo -e "${RED}✗ Prime consent test FAILED${NC}"
    PRIME_PASSED=false
fi

echo ""
sleep 2

# ==============================================================================
# Test 2: Headless Batch Captures (P95 Latency)
# ==============================================================================

echo -e "${BLUE}=== Test 2: Headless Batch Captures ===${NC}"
echo -e "${YELLOW}Running 30 captures to measure P95 latency...${NC}"
echo ""

if [ "$PRIME_PASSED" = false ]; then
    echo -e "${YELLOW}Skipping (prime consent failed)${NC}"
    BATCH_PASSED=false
else
    if ./target/release/measure-capture headless-batch --captures 30 "$SOURCE_ID" > "$RESULTS_DIR/headless-batch-$TIMESTAMP.json" 2>&1; then
        echo -e "${GREEN}✓ Headless batch test PASSED${NC}"
        BATCH_PASSED=true
    else
        echo -e "${RED}✗ Headless batch test FAILED${NC}"
        BATCH_PASSED=false
    fi
fi

echo ""
sleep 2

# ==============================================================================
# Test 3: Token Rotation Overhead
# ==============================================================================

echo -e "${BLUE}=== Test 3: Token Rotation Overhead ===${NC}"
echo -e "${YELLOW}Running 10 token rotations...${NC}"
echo ""

if [ "$PRIME_PASSED" = false ]; then
    echo -e "${YELLOW}Skipping (prime consent failed)${NC}"
    ROTATION_PASSED=false
else
    if ./target/release/measure-capture token-rotation --captures 10 "$SOURCE_ID" > "$RESULTS_DIR/token-rotation-$TIMESTAMP.json" 2>&1; then
        echo -e "${GREEN}✓ Token rotation test PASSED${NC}"
        ROTATION_PASSED=true
    else
        echo -e "${RED}✗ Token rotation test FAILED${NC}"
        ROTATION_PASSED=false
    fi
fi

echo ""

# ==============================================================================
# Summary
# ==============================================================================

echo -e "${BLUE}=== Performance Suite Summary ===${NC}"
echo ""
echo "Results saved to: $RESULTS_DIR/"
echo "  - prime-consent-$TIMESTAMP.json"
echo "  - headless-batch-$TIMESTAMP.json"
echo "  - token-rotation-$TIMESTAMP.json"
echo ""

echo "Test Results:"
if [ "$PRIME_PASSED" = true ]; then
    echo -e "  ${GREEN}✓ Prime Consent${NC}"
else
    echo -e "  ${RED}✗ Prime Consent${NC}"
fi

if [ "$BATCH_PASSED" = true ]; then
    echo -e "  ${GREEN}✓ Headless Batch (P95)${NC}"
else
    echo -e "  ${RED}✗ Headless Batch (P95)${NC}"
fi

if [ "$ROTATION_PASSED" = true ]; then
    echo -e "  ${GREEN}✓ Token Rotation${NC}"
else
    echo -e "  ${RED}✗ Token Rotation${NC}"
fi

echo ""

# Exit code
if [ "$PRIME_PASSED" = true ] && [ "$BATCH_PASSED" = true ] && [ "$ROTATION_PASSED" = true ]; then
    echo -e "${GREEN}=== ALL TESTS PASSED ===${NC}"
    exit 0
else
    echo -e "${RED}=== SOME TESTS FAILED ===${NC}"
    exit 1
fi
