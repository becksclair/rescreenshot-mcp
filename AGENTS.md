# AGENTS.md - screenshot-mcp

Cross-platform screenshot capture MCP server written in Rust.

## Quick Start

```bash
# Build
cargo build --release

# Run tests
cargo test --all-features

# Full CI check
cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features
```

## Project Structure

- `crates/screenshot-core/` - Core library with platform backends (Wayland, X11, Windows)
- `crates/screenshot-mcp-server/` - MCP server implementation using rmcp SDK
- `crates/screenshot-cli/` - Command-line interface
- `crates/screenshot-test-utils/` - Shared test utilities for integration tests

## Key Files

- `capture/mod.rs` - Backend auto-detection and facade
- `capture/wayland_backend.rs` - Wayland via xdg-desktop-portal + PipeWire
- `capture/x11_backend.rs` - X11 via xcap
- `capture/windows_backend.rs` - Windows via WGC API
- `capture/matching.rs` - Window matching strategies (regex, substring, fuzzy)
- `capture/constants.rs` - Centralized timeout constants
- `mcp.rs` - MCP tool handlers (health_check, list_windows, capture_window)

## Code Patterns

- Use `CaptureResult<T>` for error handling
- All `CaptureError` variants must have `remediation_hint()` with actionable guidance
- Thread-local regex caching via `get_or_compile_regex()` in matching.rs
- ReDoS protection: max 1MB pattern size, max 10 repetition operators

## Testing

```bash
# Run specific test
cargo test test_name -- --nocapture

# Windows-only tests
cargo test windows_backend -- --nocapture

# With all features
cargo test --all-features
```

## Test Utilities

For integration tests, use the `screenshot-test-utils` crate:

```toml
[dev-dependencies]
screenshot-test-utils = { path = "../screenshot-test-utils" }
```

**Available modules:**

- `screenshot_test_utils::windows` - Windows test fixtures (`WindowsTestContext`, `save_test_image`, `validate_image_pixels`, `find_best_target_window`)
- `screenshot_test_utils::wayland` - Wayland test harness (`create_test_backend_with_store`, `setup_test_token`, `cleanup_test_tokens`)
- `screenshot_test_utils::timing` - Cross-platform timing utilities (`measure_sync`, `assert_duration_below`, re-exports from `screenshot_core::perf`)

**MCP-specific testing** uses `tests/common/mcp_harness.rs` in the server crate (depends on `ScreenshotMcpServer`).

## Commands

- Build: `cargo build --release` or `just build`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings` or `just lint`
- Format: `cargo fmt` or `just fmt`
- Test all: `cargo test --all-features` or `just test-all`
- Test single: `cargo test TEST_NAME -- --nocapture` or `just test-one TEST_NAME`
- Full CI check: `just ci` (format + clippy + build + tests)

## Architecture

- Rust MCP server using `rmcp` SDK for stdio-based Model Context Protocol
- Modules: `capture/` (platform backends), `mcp.rs` (MCP handler), `model.rs` (types), `error.rs` (error types with remediation hints), `util/`, `perf/` (performance testing)
- Feature flags in screenshot-core: `image-processing` (default), `perf-tests`, `integration-tests`
- Test utilities in separate `screenshot-test-utils` crate (no feature flags needed)

## Documentation

- `docs/usage.md` — API reference, tool parameters, workflows
- `docs/setup.md` — Platform installation (Linux/Windows)
- `docs/troubleshooting.md` — Common errors and fixes
- `docs/architecture.md` — Backend internals, diagrams, performance
- `docs/development.md` — Testing, CI, releases, project structure
- `specs/01-specification-v1.0.md` — Formal protocol specification

## Code Style

- Edition 2024, MSRV 1.85, max line width 100
- Imports: group by std/external/crate, use `imports_granularity = "Crate"`
- Error handling: use `thiserror` for error types, `anyhow` for general errors; `CaptureResult<T>` alias
- All errors must include `remediation_hint()` method with actionable user guidance
- No `.unwrap()` or `.expect()` outside tests; use proper error propagation
- Doc comments required for public items; wrap at 80 chars
