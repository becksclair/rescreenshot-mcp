# Changelog

All notable changes to screenshot-mcp will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- **Dependencies:** Updated Linux platform dependencies for future Rust edition compatibility
  - `ashpd` 0.7.0 → 0.12.0 (fixes never type fallback warnings)
  - `xcap` 0.0.10 → 0.7.1 (requires libpipewire-0.3 for linux-x11 feature)

  **Note:** Building with `--all-features` requires `libpipewire-0.3-dev` system package. Default build (without platform-specific features) continues to work without additional system dependencies.

### Planned for v1.0.0
- Window enumeration tools (`list_windows`)
- Window capture tools (`capture_window`, `prime_wayland_consent`)
- Image format support (PNG, WebP, JPEG)
- Wayland restore token persistence
- X11 direct capture
- Windows Graphics Capture support
- macOS ScreenCaptureKit support
- ResourceLink file outputs
- CI/CD pipeline
- Release binaries

## [0.1.0] - 2025-10-13

### Added (M0: Project Scaffold)

#### Core Infrastructure
- **Project Structure:** Complete Cargo workspace with modular architecture
- **MCP Server:** stdio-based MCP server using rmcp 0.3.2 SDK
- **Platform Detection:** Automatic OS and display backend detection
  - Linux: Wayland vs X11 detection via environment variables
  - Windows: Platform identification for Win10/11
  - macOS: Platform identification for macOS 12+
- **Configuration:** rustfmt and clippy configuration for code quality

#### MCP Tools
- **health_check:** Validates server status and returns platform information
  - Returns: `{"platform": "linux", "backend": "wayland", "ok": true}`
  - Supports: Linux (wayland/x11), Windows, macOS, unknown

#### Data Models
- **BackendType:** Enum for display backend types (None, Wayland, X11, Windows, MacOS)
- **PlatformInfo:** Struct containing OS and backend information
- **HealthCheckResponse:** Response structure for health_check tool
- Full JSON serialization/deserialization with serde
- JSON Schema support via schemars

#### Developer Experience
- **Documentation:** Comprehensive doc comments on all public APIs
  - Module-level documentation
  - Function documentation with examples
  - Data model documentation
- **Testing:** 23 comprehensive tests
  - 11 model serialization tests
  - 6 platform detection tests
  - 4 MCP service tests
  - 2 documentation tests
- **Code Quality:** Zero warnings with strict clippy rules
- **Logging:** Structured logging with tracing/tracing-subscriber
  - Configurable via RUST_LOG environment variable
  - Default level: info

#### Documentation
- **README.md:** Quick start guide with installation and usage
- **TODO.md:** Complete development roadmap with milestone tracking
- **STATUS.md:** Current project status and metrics
- **CHANGELOG.md:** This file

### Technical Details

#### Dependencies
- `rmcp` 0.3.2 - Model Context Protocol SDK
- `tokio` 1.35 - Async runtime (rt-multi-thread)
- `serde` 1.0 / `serde_json` 1.0 - Serialization
- `schemars` 0.8 - JSON Schema generation
- `tracing` 0.1 / `tracing-subscriber` 0.3 - Logging
- `thiserror` 1.0 - Error handling
- `anyhow` 1.0 - Error context

#### Platform Dependencies (Optional/Prepared)
- `ashpd` 0.7 - Wayland XDG Desktop Portal (for M2)
- `x11rb` 0.13, `xcap` 0.0.10 - X11 support (for M3)
- `windows-sys` 0.52 - Windows API (for M4)
- `objc2-screen-capture-kit` 0.2 - macOS capture (for M5)

#### Build Configuration
- Rust version: 1.75+ (tested on 1.92.0-nightly)
- Edition: 2021
- Profile: Development and Release optimizations
- Features: Platform-specific features (linux-wayland, linux-x11, windows-backend, macos-backend)

### Quality Metrics

- **Lines of Code:** 1,077
- **Test Coverage:** 23 tests, 100% pass rate
- **Build Time:** ~2 minutes (cold build)
- **Binary Size:** ~8MB (release build)
- **Memory Usage:** ~3MB (idle)
- **Clippy Warnings:** 0
- **Rustfmt Issues:** 0

### Exit Criteria Met

- ✅ `cargo build --all-features` succeeds on Linux
- ✅ `cargo test` passes all 23 tests
- ✅ `cargo clippy --all-targets --all-features -D warnings` clean
- ✅ `cargo fmt --check` passes
- ✅ MCP server starts and responds to initialize
- ✅ health_check tool callable and returns valid JSON
- ✅ Platform detection accurate for Wayland/X11
- ✅ All public APIs documented
- ✅ README with quick start guide

### Performance Baseline

| Metric | Value |
|--------|-------|
| Startup Time | <100ms |
| health_check Response Time | <10ms |
| Memory (Idle) | ~3MB |
| Binary Size (Debug) | ~45MB |
| Binary Size (Release) | ~8MB |

### Known Limitations (M0)

- **No Capture Functionality:** Only platform detection implemented
- **No Image Processing:** Image encoding planned for M1
- **No Window Enumeration:** list_windows tool planned for M2-M5
- **No Integration Tests:** Only unit tests in M0
- **No CI/CD:** Planned for M6
- **No Packaging:** Roadmap for M6

### Security Notes

- No unsafe code blocks
- No credential handling (token persistence in M2)
- Stdio transport only (no network exposure)
- No user data collection

### Breaking Changes

None (initial release)

### Migration Guide

N/A (initial release)

---

## Version History

### [0.1.0] - 2025-10-13 (M0 Complete)
- Initial project scaffold
- MCP stdio server with health_check tool
- Platform detection for Linux/Windows/macOS
- 23 comprehensive tests
- Complete documentation

---

## Upcoming Releases

### [0.2.0] - Planned for 2025-10-18 (M1)
- Core capture facade trait
- Mock backend for testing
- Image encoding pipeline (PNG/WebP/JPEG)
- Temp file management
- MCP content builders

### [0.3.0] - Planned for 2025-10-25 (M2)
- Wayland backend with XDG Desktop Portal
- Restore token persistence
- Headless capture after initial consent
- Keyring integration

### [0.4.0] - Planned for 2025-11-01 (M3)
- X11 backend with window enumeration
- xcap integration for capture
- Fuzzy window matching

### [0.5.0] - Planned for 2025-11-08 (M4)
- Windows backend with WGC
- EnumWindows integration
- Cursor inclusion support

### [0.6.0] - Planned for 2025-11-15 (M5)
- macOS backend with ScreenCaptureKit
- TCC permission handling
- CGWindowList fallback

### [1.0.0] - Planned for 2025-11-22 (M6)
- Complete feature set
- CI/CD pipeline
- Release binaries for all platforms
- Comprehensive documentation
- Packaging roadmap

---

## Contributing

Contributions will be accepted after M6 completion. Please see CONTRIBUTING.md (TBD).

## License

MIT (pending final confirmation)

---

**Changelog Maintained By:** screenshot-mcp development team
**Last Updated:** 2025-10-13
**Version:** 1.0
