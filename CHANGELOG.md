# Changelog

All notable changes to screenshot-mcp are documented in this file.

## [Unreleased]

### Planned (M4-M6)

- Windows Graphics Capture backend
- macOS ScreenCaptureKit backend
- Comprehensive documentation and release
- CI/CD pipeline and GitHub releases

---

## [M3] - 2025-11-29

### Added - X11 Backend (197 tests)

**Core Features:**
- Complete X11 window enumeration via EWMH (_NET_CLIENT_LIST)
- Multi-strategy window matching:
  - Regex pattern matching with ReDoS protection
  - Case-insensitive substring search
  - Exact WM_CLASS matching
  - Exact executable name matching
  - Fuzzy matching via SkimMatcherV2 (threshold 60)
- Direct window capture via xcap with async safety (spawn_blocking)
- Full-display capture via xcap::Screen enumeration
- Region cropping transformations
- Scale transformations (0.1-2.0 factors)
- Timeout protection (1.5s list, 2s capture)

**Implementation Details:**
- EWMH property queries: _NET_CLIENT_LIST,_NET_WM_NAME, WM_CLASS, WM_NAME, _NET_WM_PID
- UTF-8 fallback to Latin-1 for title encoding
- DoS protection: 32KB limit on property queries
- Lazy connection initialization with health checks
- Reconnect-on-error strategy for stale connections
- Thread-safe connection management via Arc<Mutex<>>
- Comprehensive error mapping with logging

**Testing & Validation:**
- 197 unit tests covering all functionality
- 6 integration tests with manual execution
- 5-layer pixel validation framework:
  - Byte content validation
  - Byte size validation (width × height × 3 minimum)
  - Pixel variation analysis (reject >60-80% uniform)
  - Region cropping validation (verify <25% reduction)
  - Scale transformation validation (verify <60% for 50% scale)
- Edge case handling: black displays, fullscreen windows, small regions
- Threading and async safety verification

**Documentation:**
- `docs/x11_backend.md`: Architecture, API, error handling, performance
- Inline code documentation with examples
- Integration test procedures
- Performance benchmarks for typical operations
- Troubleshooting guide

**Quality Metrics:**
- 197 tests passing (100%)
- Zero clippy warnings
- Full rustfmt compliance
- Comprehensive public API documentation

### Fixed

- X11 property query error handling
- Connection state management
- Timeout edge cases

### Changed

- Improved window enumeration performance
- Enhanced error messages with remediation hints

---

## [M2] - 2025-10-14

### Added - Wayland Backend (236 tests)

**Core Features:**
- XDG Desktop Portal Screencast integration
- `prime_wayland_consent` MCP tool for headless authorization
- Token-based capture after initial user consent
- Automatic token rotation after each capture (security)
- Graceful fallback to display capture + region crop on token failure
- Keyring-backed token storage with encrypted file fallback

**Token Management:**
- HKDF-SHA256 key derivation (from master password)
- ChaCha20-Poly1305 encryption with random nonces
- Automatic v1→v2 migration for backward compatibility
- Lazy keyring detection with OnceLock
- File fallback with warning logs

**Performance & Security:**
- 30s timeout for portal operations (user interaction)
- 5s timeout for PipeWire frame capture
- Atomic token rotation (create new token before discarding old)
- Support for multiple source types (monitor, window, virtual)
- Support for persistence modes (transient, explicit revoke)

**Testing & Validation:**
- 236 unit tests covering all scenarios
- 6 integration tests (feature-gated, requires Wayland)
- 5-layer image validation framework
- KeyStore CRUD operations (store, retrieve, rotate, delete)
- Token rotation across multiple captures
- Error handling (timeout, fallback, portal errors)

**Documentation:**
- `docs/prime_wayland_consent.md`: Complete user guide
- Wayland backend architecture documentation
- Token lifecycle explanation
- Performance measurement utilities

**Quality Metrics:**
- 236 tests passing (100%)
- Zero clippy warnings
- Full rustfmt compliance
- Comprehensive error handling with remediation

### Fixed

- Nonce reuse vulnerability in ChaCha20-Poly1305
- Keyring initialization overhead (lazy detection)
- Lock contention in concurrent access (RwLock upgrade)

### Changed

- Upgraded from SHA-256 to HKDF-SHA256 for key derivation
- Upgraded to RwLock for ~70% better concurrent read performance

---

## [M1] - 2025-10-13

### Added - Core Capture Facade & Image Handling

**Core Features:**
- `CaptureFacade` trait with platform-agnostic interface
- `MockBackend` for testing without real display
- Image encoding pipeline supporting:
  - PNG (lossless)
  - WebP (lossy, quality 1-100)
  - JPEG (lossy, quality 1-100)
- Image transformations:
  - Region cropping (specify x, y, width, height)
  - Scale transformations (0.1-2.0 multipliers)
- Temp file management:
  - Automatic cleanup on process exit
  - Timestamped unique filenames
  - User-accessible file:// URLs
- MCP content builders:
  - Inline image blocks (base64-encoded PNG data)
  - ResourceLinks with MIME types and size
- Comprehensive error types with remediation hints

**Implementation:**
- Async/await throughout using tokio
- Serde + schemars for JSON Schema generation
- Thiserror for typed error handling
- Tracing for structured logging

**Testing:**
- 174 unit tests covering all paths
- Image encoding quality validation
- Temp file persistence and cleanup
- Error handling for all scenarios

**Quality Metrics:**
- All tests passing
- Zero warnings
- Full API documentation

---

## [M0] - 2025-10-13

### Added - Project Scaffold & MCP Server

**Core Features:**
- Cargo workspace with production-ready structure
- MCP stdio transport via rmcp SDK
- Platform detection:
  - Linux: Wayland (via $WAYLAND_DISPLAY)
  - Linux: X11 (via $DISPLAY)
  - Windows: Native detection
  - macOS: Native detection
- `health_check` MCP tool:
  - Returns platform and backend information
  - JSON response format
  - Helps verify server initialization
- Error framework with thiserror
- Structured logging with tracing
- Type-safe serialization with serde + schemars

**Project Structure:**
- `src/main.rs`: Entry point with stdio transport
- `src/mcp.rs`: MCP tool router
- `src/model.rs`: Data types and serialization
- `src/error.rs`: Error definitions
- `src/util/detect.rs`: Platform detection logic
- `tests/`: Test infrastructure
- `docs/`, `examples/`: Documentation and examples

**Configuration:**
- `Cargo.toml`: Dependencies and feature gates
- `rustfmt.toml`: Code formatting rules
- `clippy.toml`: Linter configuration
- `.gitignore`: Git ignore patterns

**Testing:**
- 23 unit tests covering:
  - Platform detection accuracy
  - MCP protocol compliance
  - Serialization/deserialization
  - Health check responses

**Documentation:**
- README.md: Quick start and overview
- docs/API.md: Tool specifications
- TODO.md: Development roadmap

**Quality:**
- Zero clippy warnings
- Full rustfmt compliance
- Comprehensive inline documentation
- All tests passing

---

## Conventions

- **Added:** New features
- **Fixed:** Bug fixes
- **Changed:** Changes to existing functionality
- **Removed:** Deleted features or code
- **Security:** Security vulnerability fixes

---

## Versioning

This project follows [Semantic Versioning](https://semver.org/):

- **MAJOR**: Breaking changes to public API
- **MINOR**: New features (backward compatible)
- **PATCH**: Bug fixes (backward compatible)

---

**Current Version:** M3 (1.0.0-alpha)
**Next Release:** M4 (Windows backend)
