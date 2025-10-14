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
# - Valid restore token (run prime_wayland_consent first)
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

# Run performance measurements
echo -e "${BLUE}Running performance measurements...${NC}"
echo -e "${YELLOW}NOTE: measure-capture is a stub (Task 11). Full implementation pending.${NC}"
echo ""

# For now, just run the stub
./target/release/measure-capture

echo ""
echo -e "${YELLOW}=== Performance Suite Status ===${NC}"
echo -e "${YELLOW}The measure-capture tool is currently a stub.${NC}"
echo -e "${YELLOW}Full implementation will measure:${NC}"
echo "  - Prime consent duration (target: <5s)"
echo "  - Capture latency P95 (target: <2s)"
echo "  - Token rotation overhead (target: <100ms)"
echo ""
echo -e "${BLUE}For manual performance validation, use integration tests:${NC}"
echo "  ./scripts/run_wayland_integration_tests.sh test_prime_consent_success"
echo "  ./scripts/run_wayland_integration_tests.sh test_capture_window_after_prime"
