#!/usr/bin/env bash
# Wayland Integration Test Runner
#
# Runs all #[ignore] integration tests that require a live Wayland session.
# These tests verify end-to-end workflows with the XDG Desktop Portal.
#
# Prerequisites:
# - Running Wayland compositor (GNOME, KDE Plasma, Sway, etc.)
# - xdg-desktop-portal and backend installed
# - WAYLAND_DISPLAY environment variable set
# - PipeWire runtime available
#
# Usage:
#   ./scripts/run_wayland_integration_tests.sh [TEST_FILTER]
#
# Examples:
#   ./scripts/run_wayland_integration_tests.sh                    # Run all integration tests
#   ./scripts/run_wayland_integration_tests.sh test_prime         # Run only prime consent tests
#   ./scripts/run_wayland_integration_tests.sh test_full_workflow # Run workflow tests

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Wayland Integration Test Runner ===${NC}"
echo ""

# Check Wayland environment
if [ -z "${WAYLAND_DISPLAY:-}" ]; then
    echo -e "${RED}ERROR: WAYLAND_DISPLAY not set. Are you running on Wayland?${NC}"
    echo "Try: echo \$WAYLAND_DISPLAY"
    exit 1
fi

echo -e "${GREEN}✓ WAYLAND_DISPLAY: $WAYLAND_DISPLAY${NC}"

# Check DBus session
if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
    echo -e "${YELLOW}WARNING: DBUS_SESSION_BUS_ADDRESS not set. Portal may not work.${NC}"
else
    echo -e "${GREEN}✓ DBUS_SESSION_BUS_ADDRESS is set${NC}"
fi

# Check XDG_SESSION_TYPE
if [ "${XDG_SESSION_TYPE:-}" = "wayland" ]; then
    echo -e "${GREEN}✓ XDG_SESSION_TYPE: wayland${NC}"
else
    echo -e "${YELLOW}WARNING: XDG_SESSION_TYPE is not 'wayland' (got: ${XDG_SESSION_TYPE:-unknown})${NC}"
fi

echo ""

# Determine test filter
TEST_FILTER="${1:-}"
if [ -n "$TEST_FILTER" ]; then
    echo -e "${BLUE}Running tests matching: $TEST_FILTER${NC}"
else
    echo -e "${BLUE}Running all integration tests${NC}"
fi

echo ""

# Build with integration-tests feature
echo -e "${BLUE}Building integration tests...${NC}"
cargo build --tests --features integration-tests --quiet

# Run tests with --ignored flag (runs only #[ignore] tests)
echo -e "${BLUE}Executing integration tests (user interaction may be required)...${NC}"
echo ""

if [ -n "$TEST_FILTER" ]; then
    cargo test --features integration-tests -- --ignored "$TEST_FILTER"
else
    cargo test --features integration-tests -- --ignored
fi

EXIT_CODE=$?

echo ""
if [ $EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}=== All integration tests passed! ===${NC}"
else
    echo -e "${RED}=== Some integration tests failed (exit code: $EXIT_CODE) ===${NC}"
    echo ""
    echo -e "${YELLOW}Common issues:${NC}"
    echo "  - Portal permission denied: Grant permission when prompted"
    echo "  - Portal timeout: Respond to the dialog within 30 seconds"
    echo "  - Token invalid: Run prime_wayland_consent first to store a valid token"
    echo "  - Portal unavailable: Install xdg-desktop-portal and your compositor's backend"
fi

exit $EXIT_CODE
