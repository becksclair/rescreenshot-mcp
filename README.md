# screenshot-mcp

> **Status:** M0 Complete (Project Scaffold) ✅
> Cross-platform screenshot MCP server for coding agents

A production-grade Model Context Protocol (MCP) stdio server that enables coding agents (Claude, Cursor, etc.) to capture application windows and return screenshots programmatically across Linux (Wayland/X11), Windows, and macOS.

## Features (M0 - Completed)

- ✅ **MCP Protocol Integration:** Full stdio transport support
- ✅ **Platform Detection:** Automatic detection of OS and display backend
- ✅ **health_check Tool:** Validates server status and platform capabilities
- ✅ **Cross-Platform:** Linux (Wayland/X11), Windows 10/11, macOS 12+
- ✅ **Type-Safe:** Comprehensive data models with serde/schemars support
- ✅ **Well-Tested:** 23 tests with 100% pass rate
- ✅ **Production-Ready Code:** Clippy clean, rustfmt formatted

## Quick Start

### Requirements

- Rust 1.75+ (tested on 1.92.0-nightly)
- Linux (Fedora/Ubuntu/Arch), Windows 10/11, or macOS 12+

### Installation

```bash
# Clone the repository
git clone https://github.com/username/screenshot-mcp.git
cd screenshot-mcp

# Build the project
cargo build --release

# Run tests
cargo test
```

### Running the Server

```bash
# Start the MCP server (stdio transport)
./target/release/screenshot-mcp

# Set log level (optional)
RUST_LOG=screenshot_mcp=debug ./target/release/screenshot-mcp
```

### MCP Client Configuration

#### Claude Desktop

Edit `~/.config/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "screenshot": {
      "command": "/path/to/screenshot-mcp",
      "args": [],
      "env": {
        "RUST_LOG": "screenshot_mcp=info"
      }
    }
  }
}
```

#### Cursor

Edit your Cursor MCP config:

```json
{
  "mcp": {
    "servers": {
      "screenshot": {
        "command": "/path/to/screenshot-mcp"
      }
    }
  }
}
```

## Available Tools

### `health_check`

Checks server health and detects the current platform and display backend.

**Request:**
```json
{
  "method": "tools/call",
  "params": {
    "name": "health_check",
    "arguments": {}
  }
}
```

**Response:**
```json
{
  "platform": "linux",
  "backend": "wayland",
  "ok": true
}
```

**Supported backends:**
- `wayland` - Linux with Wayland compositor
- `x11` - Linux with X11 display server
- `windows` - Windows 10/11
- `macos` - macOS 12+
- `none` - No display backend detected

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_health_check
```

### Code Quality

```bash
# Run clippy (strict mode)
cargo clippy --all-targets --all-features -- -D warnings

# Format code
cargo fmt

# Check formatting
cargo fmt -- --check
```

### Building Documentation

```bash
# Generate and open docs
cargo doc --open
```

## Project Structure

```
screenshot-mcp/
├── src/
│   ├── main.rs          # Entry point with stdio transport
│   ├── lib.rs           # Library root
│   ├── mcp.rs           # MCP server with health_check tool
│   ├── model.rs         # Data types and serialization
│   └── util/
│       ├── mod.rs       # Utility modules
│       └── detect.rs    # Platform detection logic
├── tests/               # Integration tests (future)
├── Cargo.toml           # Dependencies and configuration
├── TODO.md              # Development roadmap
└── README.md            # This file
```

## Roadmap

### ✅ M0: Project Scaffold (Complete)
- Project structure and configuration
- Platform detection
- MCP stdio server with `health_check` tool
- Comprehensive test suite

### 🚧 M1: Core Capture Facade (Next)
- Image encoding pipeline (PNG/WebP/JPEG)
- Temp file management
- MCP content builders

### 📅 M2-M5: Platform Backends
- **M2:** Wayland with restore tokens
- **M3:** X11 capture
- **M4:** Windows Graphics Capture
- **M5:** macOS ScreenCaptureKit

### 📅 M6: Polish & Release
- Documentation
- CI/CD
- Packaging

## Architecture

screenshot-mcp uses a modular architecture with platform-specific backends:

```
MCP Client (Claude/Cursor)
      ↓ (stdio)
screenshot-mcp Server
      ↓
Platform Detection
      ↓
┌─────┴─────┬────────┬──────────┐
Wayland    X11    Windows    macOS
Backend   Backend  Backend   Backend
```

Each backend implements the `CaptureFacade` trait for consistent cross-platform behavior.

## Technical Details

- **MCP SDK:** rmcp 0.3.2 (official Rust MCP SDK)
- **Async Runtime:** Tokio with multi-threaded runtime
- **Serialization:** serde + schemars for JSON and JSON Schema
- **Logging:** tracing + tracing-subscriber
- **Error Handling:** thiserror for typed errors

## Contributing

Contributions are welcome! Please:

1. Follow the existing code style (rustfmt)
2. Ensure all tests pass (`cargo test`)
3. Run clippy (`cargo clippy -- -D warnings`)
4. Add tests for new functionality

## License

MIT (pending final confirmation)

## Acknowledgments

- Built with [rmcp](https://github.com/4t145/rmcp) - Rust MCP SDK
- Follows the [Model Context Protocol](https://modelcontextprotocol.io/) specification
- Inspired by the need for headless screenshot capabilities on Wayland

---

**Status:** M0 Complete (2025-10-13)
**Test Coverage:** 23 tests passing
**Code Quality:** Clippy clean, rustfmt formatted
**Next Milestone:** M1 - Core Capture Facade
