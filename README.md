# screenshot-mcp

> **Status:** M2 Complete (Wayland Backend) ✅
> Cross-platform screenshot MCP server for coding agents

A production-grade Model Context Protocol (MCP) stdio server that enables coding agents (Claude, Cursor, etc.) to capture application windows and return screenshots programmatically across Linux (Wayland/X11), Windows, and macOS.

## Features

### ✅ M2 - Wayland Backend (Complete)
- **Headless Capture:** Permission-based window capture with restore tokens
- **Prime Consent:** `prime_wayland_consent` tool for first-time authorization
- **Secure Token Storage:** Platform keyring with encrypted file fallback
- **Graceful Fallback:** Display capture + region crop when restore fails
- **Performance:** <2s capture latency (P95), <5s prime consent flow
- **Production-Ready:** 236 tests passing, comprehensive error handling

### ✅ M0 - Project Scaffold (Complete)
- **MCP Protocol Integration:** Full stdio transport support
- **Platform Detection:** Automatic detection of OS and display backend
- **Type-Safe:** Comprehensive data models with serde/schemars support

## Quick Start

### Requirements

- **Rust:** 1.75+ (tested on 1.92.0-nightly)
- **Platform:** Linux (Fedora/Ubuntu/Arch), Windows 10/11, or macOS 12+

### System Dependencies

#### All Platforms

- **Build tools:** GCC, make, pkg-config
- **Rust toolchain:** `rustup` (https://rustup.rs/)

#### Linux - Build Dependencies

**Ubuntu/Debian:**
```bash
# Base build tools
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev

# X11 backend support (M3+)
sudo apt-get install -y \
  libx11-dev \
  libxcb1-dev \
  libxcb-render0-dev \
  libxcb-image0-dev

# Wayland backend support (M2+)
sudo apt-get install -y \
  libwayland-dev \
  wayland-protocols \
  libdrm-dev \
  libgbm-dev \
  libegl1-mesa-dev \
  libgl1-mesa-dev \
  libportal-dev \
  libsecret-1-dev
```

**Fedora/RHEL:**
```bash
# Base build tools
sudo dnf install -y \
  gcc \
  make \
  pkg-config \
  openssl-devel

# X11 backend support
sudo dnf install -y \
  libX11-devel \
  libxcb-devel \
  libxkbcommon-devel

# Wayland backend support
sudo dnf install -y \
  wayland-devel \
  wayland-protocols-devel \
  libdrm-devel \
  libgbm-devel \
  mesa-libEGL-devel \
  mesa-libGL-devel \
  libportal-devel \
  libsecret-devel
```

**Arch Linux:**
```bash
# Base build tools
sudo pacman -Syu
sudo pacman -S base-devel pkg-config

# X11 backend support
sudo pacman -S xorg-server xorg-xcb-util

# Wayland backend support
sudo pacman -S \
  wayland \
  wayland-protocols \
  libdrm \
  libgbm \
  mesa \
  libportal \
  libsecret
```

#### Linux - Runtime Dependencies

**Wayland Session:**
```bash
# Ubuntu/Debian
sudo apt-get install -y \
  xdg-desktop-portal \
  xdg-desktop-portal-gtk \
  pipewire

# Fedora
sudo dnf install -y \
  xdg-desktop-portal \
  xdg-desktop-portal-gtk \
  pipewire

# Arch
sudo pacman -S \
  xdg-desktop-portal \
  xdg-desktop-portal-gtk \
  pipewire
```

Verify XDG Desktop Portal is running:
```bash
systemctl --user status xdg-desktop-portal
```

**X11 Session:**
- Standard X11 display server (usually pre-installed)
- `xcap` library will be compiled from source

#### Windows

- **Visual Studio Build Tools 2019+** or **MSVC compiler** (part of Visual Studio)
- **Windows 10/11** with graphics drivers

#### macOS

- **Xcode Command Line Tools:**
  ```bash
  xcode-select --install
  ```
- **macOS 12.0+** with native graphics support

### Installation

```bash
# Clone the repository
git clone https://github.com/username/screenshot-mcp.git
cd screenshot-mcp

# Build the project
cargo build --release

# Run tests (verify everything works)
cargo test
```

### Wayland-Specific Setup

For Wayland capture support, see [Prime Wayland Consent Guide](./docs/prime_wayland_consent.md) for detailed setup.

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

### `prime_wayland_consent` (Wayland only)

Obtains user permission for headless window capture on Wayland. Required before first capture.

**Request:**
```json
{
  "method": "tools/call",
  "params": {
    "name": "prime_wayland_consent",
    "arguments": {
      "source_id": "wayland-default",
      "source_type": "monitor",
      "include_cursor": false
    }
  }
}
```

**Response:**
```json
{
  "status": "success",
  "source_id": "wayland-default",
  "num_streams": 1,
  "next_steps": "Use capture_window with this source_id"
}
```

See [Prime Wayland Consent Guide](./docs/prime_wayland_consent.md) for workflow details.

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

### Quick Start with Just

This project uses [just](https://github.com/casey/just) for common tasks:

```bash
# Install just
cargo install just

# List all available recipes
just --list

# Run tests
just test

# Run performance suite
just perf

# Run all quality checks
just check

# Run CI checks locally
just ci
```

### Common Tasks

```bash
# Testing
just test                    # Run all unit tests
just test-wayland            # Run Wayland backend tests
just test-perf               # Run with performance utilities
just test-integration        # Run integration tests (requires Wayland)

# Performance Testing
just perf-prime              # Prime consent workflow
just perf-batch              # Run 30 headless captures
just perf-rotation           # Measure token rotation
just perf-memory             # Memory profiling with valgrind

# Acceptance Testing
just accept-01               # T-M2-01: Fresh install → prime consent
just accept-02               # T-M2-02: Headless capture test
just accept-status           # Show acceptance test status

# Code Quality
just lint                    # Run clippy
just fmt                     # Format code
just check                   # Run all checks (lint + fmt + test)

# Building
just build                   # Release build
just build-all               # Build with all features
just build-perf              # Build performance tool

# Documentation
just doc                     # Build and open docs
```

### Manual Commands

If you don't have `just` installed, you can run commands directly:

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

### ✅ M1: Core Capture Facade (Complete)
- Image encoding pipeline (PNG/WebP/JPEG)
- Temp file management
- MCP content builders

### ✅ M2: Wayland Backend (Complete)
- Headless capture with restore tokens
- Prime consent workflow
- Secure token storage (keyring + file fallback)
- Graceful fallback strategy
- Performance validation tools
- 236 tests passing

### 📅 M3-M5: Additional Platform Backends
- **M3:** X11 capture (Week 4)
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

### Rust Dependencies

- **MCP SDK:** rmcp 0.8 (official Rust MCP SDK)
- **Async Runtime:** Tokio with multi-threaded runtime
- **Serialization:** serde + schemars for JSON and JSON Schema
- **Logging:** tracing + tracing-subscriber
- **Error Handling:** thiserror for typed errors
- **Image Processing:** image crate with PNG/WebP/JPEG support
- **Platform Detection:** Custom detection logic in `util/detect.rs`

### System Dependency Reference

#### Linux X11 Backend
| Dependency | Ubuntu/Debian | Fedora | Arch | Purpose |
|---|---|---|---|---|
| libx11 | `libx11-dev` | `libX11-devel` | `xorg-server` | X11 protocol |
| libxcb | `libxcb1-dev` | `libxcb-devel` | `libxcb` | X11 protocol |
| libxkbcommon | N/A | `libxkbcommon-devel` | `libxkbcommon` | Keyboard handling |

#### Linux Wayland Backend
| Dependency | Ubuntu/Debian | Fedora | Arch | Purpose |
|---|---|---|---|---|
| libwayland | `libwayland-dev` | `wayland-devel` | `wayland` | Wayland protocol |
| wayland-protocols | `wayland-protocols` | `wayland-protocols-devel` | `wayland-protocols` | Wayland specs |
| libdrm | `libdrm-dev` | `libdrm-devel` | `libdrm` | Graphics |
| libgbm | `libgbm-dev` | `libgbm-devel` | `libgbm` | Graphics buffer |
| libegl | `libegl1-mesa-dev` | `mesa-libEGL-devel` | `mesa` | Graphics API |
| libGL | `libgl1-mesa-dev` | `mesa-libGL-devel` | `mesa` | Graphics |
| libportal | `libportal-dev` | `libportal-devel` | `libportal` | Desktop portal |
| libsecret | `libsecret-1-dev` | `libsecret-devel` | `libsecret` | Token storage |

#### Linux Runtime
| Dependency | Ubuntu/Debian | Fedora | Arch | Purpose |
|---|---|---|---|---|
| xdg-desktop-portal | `xdg-desktop-portal` | `xdg-desktop-portal` | `xdg-desktop-portal` | Portal daemon |
| portal backend | `xdg-desktop-portal-gtk` | `xdg-desktop-portal-gtk` | `xdg-desktop-portal-gtk` | Portal UI |
| pipewire | `pipewire` | `pipewire` | `pipewire` | Audio/video server |

#### Windows
- Visual Studio Build Tools 2019+ (includes MSVC)
- Windows SDK headers (usually with Visual Studio)

#### macOS
- Xcode Command Line Tools (includes clang)
- macOS 12.0+ SDK (included with Xcode)

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

**Status:** M2 Complete (2025-10-14)
**Test Coverage:** 236 tests passing (unit + integration)
**Code Quality:** Clippy clean, rustfmt formatted
**Performance:** <2s capture latency (P95), <5s prime consent
**Next Milestone:** M3 - X11 Backend
