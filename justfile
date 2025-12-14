# screenshot-mcp justfile
# Run tasks with: just <recipe>
# List all recipes: just --list

# Default recipe (show help)
default:
    @just --list

# ============================================================================
# Building
# ============================================================================

# Build release binary
build:
    cargo build --release -p screenshot-mcp-server

# Build with all features enabled
build-all:
    cargo build --all-features --release

# Build Wayland backend
build-wayland:
    cargo build --release -p screenshot-mcp-server

# Build performance measurement tool
build-perf:
    cargo build -p screenshot-cli --bin measure-capture --features perf-tests --release

# Clean build artifacts
clean:
    cargo clean

# ============================================================================
# Testing
# ============================================================================

# Run all unit tests
test:
    cargo test

# Run unit tests with output
test-verbose:
    cargo test -- --nocapture

# Run tests with all features
test-all:
    cargo test --all-features

# Run library tests only
test-lib:
    cargo test --lib

# Run tests with performance utilities
test-perf:
    cargo test --features perf-tests

# Run specific test by name
test-one TEST:
    cargo test {{TEST}} -- --nocapture

# Run integration tests (requires live Wayland session)
test-integration:
    cargo test --test error_integration_tests -- --ignored --nocapture

# Run Wayland integration test script
test-wayland-script TEST="":
    #!/usr/bin/env bash
    if [ -z "{{TEST}}" ]; then
        ./scripts/run_wayland_integration_tests.sh
    else
        ./scripts/run_wayland_integration_tests.sh {{TEST}}
    fi

# ============================================================================
# Performance Testing
# ============================================================================

# Run complete performance test suite (requires priming)
perf:
    ./scripts/run_performance_suite.sh

# Run memory profiling (requires valgrind)
perf-memory CAPTURES="10":
    ./scripts/run_memory_probe.sh {{CAPTURES}}

# Prime Wayland consent for performance testing
perf-prime SOURCE_ID="wayland-perf":
    cargo run -p screenshot-cli --bin measure-capture --features perf-tests --release -- prime-consent {{SOURCE_ID}}

# Run headless batch captures (requires priming first)
perf-batch SOURCE_ID="wayland-perf" CAPTURES="30":
    cargo run -p screenshot-cli --bin measure-capture --features perf-tests --release -- headless-batch --captures {{CAPTURES}} {{SOURCE_ID}}

# Measure token rotation performance
perf-rotation SOURCE_ID="wayland-perf" CAPTURES="10":
    cargo run -p screenshot-cli --bin measure-capture --features perf-tests --release -- token-rotation --captures {{CAPTURES}} {{SOURCE_ID}}

# Show performance thresholds
perf-summary:
    cargo run -p screenshot-cli --bin measure-capture --features perf-tests --release -- summary

# ============================================================================
# Code Quality
# ============================================================================

# Run clippy with strict checks
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Format code
fmt:
    cargo fmt

# Check formatting without modifying files
fmt-check:
    cargo fmt -- --check

# Run all quality checks (lint + fmt-check + test)
check:
    @echo "=== Running clippy ==="
    cargo clippy --all-targets --all-features -- -D warnings
    @echo ""
    @echo "=== Checking formatting ==="
    cargo fmt -- --check
    @echo ""
    @echo "=== Running tests ==="
    cargo test --all-features

# ============================================================================
# Documentation
# ============================================================================

# Build and open documentation
doc:
    cargo doc --open --all-features

# Build documentation without opening
doc-build:
    cargo doc --all-features

# Check documentation for warnings
doc-check:
    cargo doc --all-features --no-deps 2>&1 | grep -i warning || echo "No documentation warnings"

# ============================================================================
# Running
# ============================================================================

# Run the MCP server (stdio mode)
run:
    cargo run --release -p screenshot-mcp-server

# Run server with debug logging
run-debug:
    RUST_LOG=screenshot_mcp_server=debug,screenshot_core=debug cargo run --release -p screenshot-mcp-server

# Run server with trace logging
run-trace:
    RUST_LOG=screenshot_mcp_server=trace,screenshot_core=trace cargo run --release -p screenshot-mcp-server

# ============================================================================
# Acceptance Testing
# ============================================================================

# Run acceptance test T-M2-01: Fresh install → prime consent
accept-01:
    #!/usr/bin/env bash
    echo "=== T-M2-01: Fresh Install → Prime Consent ==="
    echo "Cleaning state..."
    rm -rf ~/.local/share/screenshot-mcp/
    echo ""
    echo "Running integration test (grant permission when prompted)..."
    ./scripts/run_wayland_integration_tests.sh test_prime_consent_success

# Run acceptance test T-M2-02: Restart process → headless capture
accept-02:
    #!/usr/bin/env bash
    echo "=== T-M2-02: Restart Process → Headless Capture ==="
    echo "Running integration test..."
    ./scripts/run_wayland_integration_tests.sh test_capture_window_after_prime

# Run acceptance test T-M2-03: Compositor restart simulation
accept-03:
    #!/usr/bin/env bash
    echo "=== T-M2-03: Compositor Restart Simulation ==="
    echo "This test requires manual compositor restart."
    echo "Please logout/login between steps."
    echo ""
    read -p "Press Enter to continue..."
    ./scripts/run_wayland_integration_tests.sh test_full_workflow_compositor_restart

# Run acceptance test T-M2-04: Restore fails → fallback
accept-04:
    #!/usr/bin/env bash
    echo "=== T-M2-04: Restore Fails → Fallback ==="
    echo "Invalidating token..."
    echo "invalid-token-data" > ~/.local/share/screenshot-mcp/token-store.enc
    echo ""
    echo "Running capture (should trigger fallback)..."
    ./scripts/run_wayland_integration_tests.sh test_capture_window_no_token_fallback

# Run acceptance test T-M2-05: Keyring unavailable → file fallback
accept-05:
    @echo "=== T-M2-05: Keyring Unavailable → File Fallback ==="
    @echo "Manual test - requires disabling keyring access"
    @echo "See docs/acceptance-checklist.md for instructions"

# Show acceptance test status
accept-status:
    @echo "=== M2 Acceptance Test Status ==="
    @echo ""
    @echo "Run tests with: just accept-01, just accept-02, etc."
    @echo ""
    @cat docs/acceptance-checklist.md | grep -A 1 "^| T-M2-"

# ============================================================================
# Utility
# ============================================================================

# Show project status
status:
    @echo "=== screenshot-mcp Project Status ==="
    @echo ""
    @echo "Milestone: M2 - Wayland Backend Complete"
    @echo ""
    @cargo test --all-features 2>&1 | grep "test result:"
    @echo ""
    @echo "Build status:"
    @cargo build --all-features --release 2>&1 | tail -1
    @echo ""
    @echo "Code quality:"
    @cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tail -1

# Show test coverage summary
coverage:
    @echo "=== Test Coverage Summary ==="
    @echo ""
    @echo "Total tests:"
    @cargo test --features perf-tests --all-targets -- --list 2>&1 | grep -c "test " || echo "0"
    @echo ""
    @echo "By category:"
    @echo -n "  Library tests: "
    @cargo test --lib -- --list 2>&1 | grep -c "test " || echo "0"
    @echo -n "  Integration tests: "
    @cargo test --test error_integration_tests -- --list 2>&1 | grep -c "test " || echo "0"
    @echo -n "  Doc tests: "
    @cargo test --doc -- --list 2>&1 | grep -c "test " || echo "0"

# Create performance results directory
init-perf:
    mkdir -p perf-results
    @echo "Created perf-results/ directory"

# Watch for changes and run tests
watch:
    cargo watch -x "test --all-features"

# Watch and run specific test
watch-test TEST:
    cargo watch -x "test {{TEST}}"

# Install required tools for development
install-tools:
    @echo "Installing development tools..."
    cargo install cargo-watch || echo "cargo-watch already installed"
    @echo ""
    @echo "System packages required:"
    @echo "  - valgrind (for memory profiling)"
    @echo "  - xdg-desktop-portal (for Wayland capture)"
    @echo "  - pipewire (for screen capture)"
    @echo ""
    @echo "Install with your package manager:"
    @echo "  Ubuntu/Debian: sudo apt install valgrind xdg-desktop-portal xdg-desktop-portal-gtk pipewire"
    @echo "  Fedora:        sudo dnf install valgrind xdg-desktop-portal xdg-desktop-portal-gtk pipewire"
    @echo "  Arch:          sudo pacman -S valgrind xdg-desktop-portal xdg-desktop-portal-gtk pipewire"

# ============================================================================
# CI/CD
# ============================================================================

# Run CI checks locally
ci:
    @echo "=== Running CI Checks ==="
    @echo ""
    @echo "1. Format check..."
    cargo fmt -- --check
    @echo ""
    @echo "2. Clippy..."
    cargo clippy --all-targets --all-features -- -D warnings
    @echo ""
    @echo "3. Build..."
    cargo build --all-features --release
    @echo ""
    @echo "4. Tests..."
    cargo test --all-features
    @echo ""
    @echo "=== All CI checks passed ==="

# Run the GitHub Actions workflow locally via nektos/act (Linux-only).
#
# Prereqs:
# - Install act: https://nektosact.com/
# - Docker available (Docker Desktop / Colima / etc.)
ci-act-list:
    act -l

ci-act-linux:
    @echo "=== Running GitHub Actions CI locally (Linux via act) ==="
    act -j test --matrix os:ubuntu-latest
    act -j build --matrix os:ubuntu-latest

ci-act-linux-test:
    act -j test --matrix os:ubuntu-latest

ci-act-linux-build:
    act -j build --matrix os:ubuntu-latest

ci-act-windows:
    @echo "=== Attempting Windows CI via act (expected to fail) ==="
    @echo "act runs Linux containers; it cannot emulate GitHub-hosted windows-latest runners."
    @echo "Use real Windows (native) for parity. This command is intentionally 'try and fail'."
    @echo "Forcing a non-zero exit to avoid false 'success' from skipped jobs."
    act -j test --matrix os:windows-latest -P windows-latest=ghcr.io/catthehacker/does-not-exist:act-latest
    act -j build --matrix os:windows-latest -P windows-latest=ghcr.io/catthehacker/does-not-exist:act-latest

ci-act-macos:
    @echo "=== Attempting macOS CI via act (expected to fail) ==="
    @echo "act runs Linux containers; it cannot emulate GitHub-hosted macos-latest runners."
    @echo "Use real macOS (native) for parity. This command is intentionally 'try and fail'."
    @echo "Forcing a non-zero exit to avoid false 'success' from skipped jobs."
    act -j test --matrix os:macos-latest -P macos-latest=ghcr.io/catthehacker/does-not-exist:act-latest
    act -j build --matrix os:macos-latest -P macos-latest=ghcr.io/catthehacker/does-not-exist:act-latest

# ============================================================================
# Release
# ============================================================================

# Prepare for release (run all checks)
release-prep:
    @echo "=== Preparing for Release ==="
    just clean
    just check
    just doc-build
    just build-all
    @echo ""
    @echo "=== Release preparation complete ==="
    @echo "Ready to tag and release"
