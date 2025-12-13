# AGENTS.md - screenshot-mcp

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
- Feature flags: `linux-wayland`, `linux-x11`, `windows-backend`, `macos-backend`, `image-processing` (default)

## Code Style

- Edition 2024, MSRV 1.75, max line width 100
- Imports: group by std/external/crate, use `imports_granularity = "Crate"`
- Error handling: use `thiserror` for error types, `anyhow` for general errors; `CaptureResult<T>` alias
- All errors must include `remediation_hint()` method with actionable user guidance
- No `.unwrap()` or `.expect()` outside tests; use proper error propagation
- Doc comments required for public items; wrap at 80 chars
