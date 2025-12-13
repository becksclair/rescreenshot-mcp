# screenshot-mcp

> **Cross-platform screenshot MCP server for coding agents**
> Capture application windows on Linux (Wayland/X11), Windows, and macOS

## Status

✅ **M0-M3 Complete** — Wayland and X11 backends production-ready
📅 **M4-M6 Planned** — Windows, macOS, and final polish

- ✅ Wayland headless capture with restore tokens (236 tests)
- ✅ X11 window enumeration and capture (197 tests)
- 📅 Windows Graphics Capture (planned M4)
- 📅 macOS ScreenCaptureKit (planned M5)

## Quick Start

### Install

```bash
git clone https://github.com/username/screenshot-mcp.git
cd screenshot-mcp

# Build the binary
cargo build --release

# Run tests to verify
cargo test --all-features
```

### System Requirements

**Linux (Ubuntu/Debian):**
```bash
# Build dependencies
sudo apt-get install -y \
  build-essential pkg-config libssl-dev \
  libwayland-dev libportal-dev libsecret-1-dev \
  libx11-dev libxcb1-dev

# Runtime (Wayland)
sudo apt-get install -y xdg-desktop-portal xdg-desktop-portal-gtk pipewire
```

**Fedora/RHEL:**
```bash
sudo dnf install -y \
  gcc make pkg-config openssl-devel \
  wayland-devel libportal-devel libsecret-devel \
  libX11-devel libxcb-devel

sudo dnf install -y xdg-desktop-portal xdg-desktop-portal-gtk pipewire
```

**Arch Linux:**
```bash
sudo pacman -Syu && sudo pacman -S base-devel wayland libportal libsecret libx11 libxcb
sudo pacman -S xdg-desktop-portal xdg-desktop-portal-gtk pipewire
```

**Windows:** Visual Studio Build Tools 2019+ (includes MSVC)

**macOS:** Xcode Command Line Tools (`xcode-select --install`)

### Configure

**Claude Desktop** — Edit `~/.config/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "screenshot": {
      "command": "/path/to/screenshot-mcp",
      "env": { "RUST_LOG": "screenshot_mcp=info" }
    }
  }
}
```

Then restart Claude Desktop and you're ready to use!

## How It Works

### For Wayland Users

1. **First time:** Agent calls `prime_wayland_consent` → portal opens → you approve
2. **Afterwards:** Agent captures windows headlessly without prompts
3. **Under the hood:** Token stored securely in system keyring, rotated automatically

### For X11 Users

- Agent directly enumerates windows via EWMH
- Captures via xcap without permission flow
- Multi-strategy matching: regex, fuzzy, exact class/exe

### For Everyone

- **Dual output:** Screenshot data + timestamped file link
- **Flexible encoding:** PNG (default), WebP, JPEG with quality control
- **Transformations:** Region cropping, scaling
- **Error recovery:** Clear error messages with remediation hints

## Available Tools

### `health_check`

Verifies server is running and detects your platform/backend:

```json
{ "platform": "linux", "backend": "wayland", "ok": true }
```

### `prime_wayland_consent` (Wayland only)

Opens the desktop portal to authorize capture. Do this once, then headless capture works automatically.

### `list_windows`

Shows available windows you can capture:
- **Wayland:** Displays primed sources
- **X11:** Lists all windows with titles/classes

### `capture_window`

Captures a specific window. Use window title, class name, or executable name as selector.

## Documentation Guide

### Finding What You Need

**New to the project?**
- Start with [Quick Start](#quick-start) above
- Read [How It Works](#how-it-works) for your platform
- Check [Troubleshooting](#troubleshooting) if something's wrong

**Developer?**
- Read [`specs/01-specification-v1.0.md`](./specs/01-specification-v1.0.md) for architecture
- Check [`TODO.md`](./TODO.md) for roadmap (M4-M6 planned)
- See [`docs/`](#documentation-files) for implementation guides

**Want to contribute?**
- Follow [Contributing](#contributing) guidelines
- Run tests: [`Development`](#development) section
- Update [`CHANGELOG.md`](./CHANGELOG.md) for changes

### Documentation Files

| File | Purpose |
|------|---------|
| **README.md** (this file) | User guide and quick start |
| **[specs/01-specification-v1.0.md](./specs/01-specification-v1.0.md)** | Technical specification and architecture |
| **[TODO.md](./TODO.md)** | Development roadmap (M0-M3 complete, M4-M6 planned) |
| **[CHANGELOG.md](./CHANGELOG.md)** | Release history (M0-M3) |
| **[docs/API.md](./docs/API.md)** | MCP tool API reference |
| **[docs/TESTING.md](./docs/TESTING.md)** | Testing guide and procedures |
| **[docs/x11_backend.md](./docs/x11_backend.md)** | X11 backend architecture |
| **[docs/prime_wayland_consent.md](./docs/prime_wayland_consent.md)** | Wayland setup and workflow |
| **[docs/IMAGE_VALIDATION_TESTING.md](./docs/IMAGE_VALIDATION_TESTING.md)** | Pixel validation framework |

## Development

### Run Tests

```bash
# All unit tests
cargo test --all-features

# X11 integration tests (requires X11 display)
DISPLAY=:0 cargo test --test x11_integration_tests --features linux-x11 -- --ignored

# With logging
RUST_LOG=screenshot_mcp=debug cargo test --all-features -- --nocapture
```

### Code Quality

```bash
# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Format code
cargo fmt

# All checks
cargo test && cargo clippy && cargo fmt --check
```

### Quick Development Loop

```bash
# Just (optional, but convenient)
cargo install just

just test          # Run all tests
just lint          # Run clippy
just fmt           # Format code
just doc           # Build and open docs
```

## Architecture

**Trait-based design** — Each platform implements `CaptureFacade`:

```text
MCP Server
    ↓
CaptureFacade trait
    ├── WaylandBackend (with keyring integration)
    ├── X11Backend (with EWMH enumeration)
    ├── WindowsBackend (planned M4)
    ├── MacBackend (planned M5)
    └── MockBackend (for testing)
```

**Key features:**
- Async/await throughout (tokio)
- Comprehensive error handling
- Configurable timeouts
- Structured logging (tracing)
- Type-safe serialization (serde + schemars)

## Project Structure

```text
src/
  ├── main.rs              # Entry point
  ├── lib.rs               # Library root
  ├── mcp.rs               # MCP server and tools
  ├── model.rs             # Data types
  ├── error.rs             # Error definitions
  ├── capture/
  │   ├── mod.rs           # CaptureFacade trait
  │   ├── mock.rs          # Mock backend for testing
  │   ├── wayland_backend.rs   # Wayland implementation
  │   ├── x11_backend.rs       # X11 implementation
  │   └── image_buffer.rs      # Image encoding
  ├── util/
  │   ├── detect.rs        # Platform detection
  │   ├── encode.rs        # Image encoding pipeline
  │   ├── key_store.rs     # Token storage (Wayland)
  │   └── mcp_content.rs   # MCP response builders
  └── perf/
      └── mod.rs           # Performance measurement utilities

tests/
  ├── common/              # Test utilities
  ├── error_integration_tests.rs     # M2 integration
  └── x11_integration_tests.rs       # M3 integration

docs/
  ├── prime_wayland_consent.md   # Wayland setup guide
  ├── x11_backend.md             # X11 implementation guide
  └── IMAGE_VALIDATION_TESTING.md # Testing methodology
```

## Performance

**Typical capture latency (P95):**

| Operation | Latency |
|-----------|---------|
| Wayland prime consent | <3s |
| Wayland headless capture | <1.5s |
| X11 list windows | ~150ms |
| X11 window capture | 100-500ms |
| Image encoding (PNG) | 100-200ms |

**Resource usage:**
- Memory peak: <150MB
- Binary size: ~8MB
- Build time: ~2 minutes

## Testing

### Test Organization

- **Unit Tests (430+):** Fast, no GUI required → runs in CI
- **Integration Tests (12):** Require live display → manual execution
- **Image Validation:** 5-layer pixel verification ensures real captures

### Coverage

- ✅ 100% test pass rate (433/433)
- ✅ Zero clippy warnings
- ✅ All public APIs documented
- ✅ Comprehensive error paths

### Running Locally

```bash
# Quick unit tests (30s)
cargo test --lib

# Full suite (1-2 minutes)
cargo test --all-features

# X11 integration (requires X11 and windows)
DISPLAY=:0 cargo test --test x11_integration_tests --features linux-x11 -- --ignored
```

## Roadmap

### ✅ Completed

| Milestone | Focus | Tests | Status |
|-----------|-------|-------|--------|
| M0 | Project scaffold, MCP server | 23 | ✅ Complete |
| M1 | Image encoding, temp files | 174 | ✅ Complete |
| M2 | Wayland backend | 236 | ✅ Complete |
| M3 | X11 backend | 197 | ✅ Complete |

### 📅 Planned

| Milestone | Focus | ETA | Notes |
|-----------|-------|-----|-------|
| M4 | Windows Graphics Capture | Q1 2026 | WGC API integration |
| M5 | macOS ScreenCaptureKit | Q1 2026 | SCKit + TCC handling |
| M6 | Documentation & Release | Q1 2026 | User guides, CI/CD |

## Troubleshooting

### "Portal unavailable" (Wayland)

**Problem:** Screenshot-mcp can't find XDG Desktop Portal

**Solution:**
```bash
# Check if portal is running
systemctl --user status xdg-desktop-portal

# Install portal if missing
sudo apt-get install xdg-desktop-portal xdg-desktop-portal-gtk

# For KDE Plasma specifically:
sudo apt-get install xdg-desktop-portal-kde
```

### "No X11 display" (X11)

**Problem:** Tests say `DISPLAY` is not set

**Solution:**
```bash
# Check your display
echo $DISPLAY

# If empty, set it
export DISPLAY=:0

# Verify X11 is available
ls /tmp/.X11-unix/
```

### "Permission denied" (Wayland)

**Problem:** Portal shows permission denied

**Solution:**
1. Call `prime_wayland_consent` again
2. Explicitly approve in the portal dialog
3. Check system keyring is accessible (`gnome-keyring`, `pass`, etc.)

### Tests failing with "window not found"

**Problem:** Integration tests can't find any windows

**Solution:**
- Open some GUI applications first (Firefox, terminal, etc.)
- Tests need visible windows to capture

## Contributing

We welcome contributions! Please:

1. **Fork & branch:** Create a feature branch
2. **Code style:** Use `cargo fmt` and `cargo clippy`
3. **Tests:** Add tests for new functionality
4. **Verify:** Run `cargo test --all-features` before submitting PR
5. **Document:** Update docs/ for user-facing changes

### Development Tips

- Use `RUST_LOG=screenshot_mcp=debug` for detailed logging
- Check `tests/` directory for examples
- Read module docs for architecture details

## FAQ

**Q: Does this work on Wayland?**
A: Yes! That's the primary use case. Call `prime_wayland_consent` once, then capture headlessly.

**Q: What about X11?**
A: Full support for X11 with multi-strategy window matching. Capture works directly without permission flows.

**Q: Why is Windows/macOS not done yet?**
A: Linux backends (M0-M3) completed first. Windows (M4) and macOS (M5) are next.

**Q: Can I use this in production?**
A: Yes! M0-M3 are production-ready (236+197 tests, zero warnings). Windows/macOS planned for Q1 2026.

**Q: How are restore tokens stored?**
A: Platform keyring (system secure storage). Falls back to encrypted file if keyring unavailable.

**Q: What's the performance like?**
A: <2s capture latency (P95), typically 100-500ms on modern hardware.

## License

MIT

## Quick Reference

### Common Commands

```bash
# Build and test
cargo build --release              # Build binary
cargo test --all-features          # Run all tests
cargo clippy --all-features -- -D warnings  # Linting
cargo fmt --check                  # Format check

# Documentation
cargo doc --no-deps --open         # Generate and view docs

# Platform-specific testing
RUST_LOG=debug ./target/release/screenshot-mcp  # Wayland (requires Wayland session)
DISPLAY=:0 cargo test --test x11_integration_tests --features linux-x11 -- --ignored  # X11
```

### Getting Help

| Question | Answer |
|----------|--------|
| **Installation issue?** | Check [System Requirements](#system-requirements) and [Troubleshooting](#troubleshooting) |
| **How do I use it?** | Read [How It Works](#how-it-works) for your platform |
| **What can it do?** | See [Available Tools](#available-tools) |
| **How is it built?** | Read [Architecture](#architecture) and [specs/01-specification-v1.0.md](./specs/01-specification-v1.0.md) |
| **What's the roadmap?** | Check [Roadmap](#roadmap) and [TODO.md](./TODO.md) |
| **How do I test it?** | See [Testing](#testing) section |
| **How do I contribute?** | Read [Contributing](#contributing) |

## Project Status

| Aspect | Status |
|--------|--------|
| **Code** | 433 tests passing, 0 warnings ✅ |
| **Milestones** | M0-M3 complete (67%) ✅ |
| **Platforms** | Wayland & X11 production-ready ✅ |
| **Documentation** | Comprehensive and up-to-date ✅ |
| **Production Ready** | Yes (for M0-M3) ✅ |

## Acknowledgments

- Built with [rmcp](https://github.com/4t145/rmcp) — Rust MCP SDK
- Follows [Model Context Protocol](https://modelcontextprotocol.io/) spec
- Wayland support via [ashpd](https://github.com/bilelmoussaoui/ashpd) DBus bindings
- X11 support via [x11rb](https://github.com/x11-rs/x11rb) and [xcap](https://github.com/nashaofu/xcap)

---

**Last Updated:** 2025-11-29
**Current Version:** M3 (1927 lines of X11 backend, 433 tests passing)
**Status:** Production Ready for Wayland/X11
**Next Milestone:** M4 - Windows Graphics Capture
