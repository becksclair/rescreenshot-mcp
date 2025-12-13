# screenshot-mcp Development Roadmap

## Project Status

**Current:** M0-M4 Complete ✅ | M6a Planned | M5 Deferred
**Code Quality:** 500+ tests passing (73 unit + 21 integration for Windows), 0 warnings, production-ready
**Last Updated:** 2025-12-13

**Important:** All testing and verification steps throughout this roadmap must be executed automatically by the coding agent. Manual testing steps should be converted to automated tests where possible.

---

## Completed Milestones ✅

### M0: Project Scaffold & MCP Server (Complete)

- [x] Cargo workspace and project structure
- [x] MCP stdio transport via rmcp SDK
- [x] Platform detection (Wayland/X11/Windows/macOS)
- [x] `health_check` tool with JSON response
- [x] Comprehensive error framework
- [x] 23 unit tests passing
- [x] Zero warnings, fully formatted code

### M1: Core Capture Facade & Image Handling (Complete)

- [x] `CaptureFacade` trait with async methods
- [x] `MockBackend` for testing
- [x] Image encoding pipeline (PNG/WebP/JPEG)
- [x] Quality and scale parameters
- [x] Temp file management with cleanup
- [x] MCP content builders (inline + ResourceLinks)
- [x] Comprehensive error types
- [x] 174 unit tests passing

### M2: Wayland Backend (Complete)

- [x] XDG Desktop Portal Screencast integration
- [x] `prime_wayland_consent` MCP tool
- [x] Keyring-backed token storage with encryption
- [x] Token rotation after each capture
- [x] Graceful fallback to display capture + crop
- [x] Comprehensive error handling
- [x] Performance validation (<2s capture)
- [x] 236 unit tests, 6 integration tests passing
- [x] 5-layer image validation framework
- [x] Complete user documentation
- [x] Feature gates (linux-wayland)

### M3: X11 Backend (Complete)

- [x] EWMH window enumeration via _NET_CLIENT_LIST
- [x] Property query helpers (UTF-8, Latin-1, WM_CLASS, PID)
- [x] Multi-strategy window resolution (regex, substring, fuzzy, exact)
- [x] xcap integration for direct window capture
- [x] Display capture via xcap::Screen
- [x] Region cropping and scale transformations
- [x] Connection management (lazy init, reconnect-on-error)
- [x] DoS protection (32KB limit on properties)
- [x] Timeout protection (1.5s list, 2s capture)
- [x] Error mapping with logging
- [x] 197 unit tests passing
- [x] 6 integration tests with pixel validation
- [x] Edge case handling (black displays, small regions, etc.)
- [x] Comprehensive architecture documentation
- [x] Feature gates (linux-x11)

### M4: Windows Graphics Capture Backend (Complete)

- [x] Windows.Graphics.Capture API integration
- [x] Win32 window enumeration (EnumWindows, GetWindowText, GetClassName)
- [x] Multi-strategy window resolution (regex, substring, fuzzy, class, exe)
- [x] Frame acquisition with BGRA→RGBA conversion
- [x] Region cropping and scale transformations
- [x] Cursor capture via WGC settings
- [x] Async-safe spawn_blocking wrapper
- [x] Timeout protection (1.5s list, 2s capture)
- [x] Windows build version checking (17134+)
- [x] Comprehensive error handling with remediation hints
- [x] 73 unit tests passing
- [x] 21 integration tests with visual verification
- [x] Shared test utilities (`WindowsTestContext`, `save_test_image`, etc.)
- [x] Windows-specific architecture documentation
- [x] Feature gates (windows-backend)

---

## Planned Milestones 📅

### M5: macOS ScreenCaptureKit Backend

**Target:** Q1 2026
**Estimated Effort:** 5-7 days
**Dependencies:** M0-M1 complete

**Scope:**

#### Phase 1: MacBackend Module Skeleton

- [ ] Create `src/capture/mac_backend.rs`
- [ ] Define `MacBackend` struct
- [ ] Implement `CaptureFacade` trait with stubs
- [ ] Add feature gate (`macos-screencapturekit`)
- [ ] Export from `src/capture/mod.rs`
- [ ] macOS 12+ detection

#### Phase 2: Window Enumeration

- [ ] Use `CGWindowListCopyWindowInfo` for enumeration
- [ ] Extract window title, owner, PID, dimensions
- [ ] Filter background windows
- [ ] Implement `list_windows()`
- [ ] TCC permission checking

#### Phase 3: Window Resolution

- [ ] Implement `resolve_target()` with matching
- [ ] Title matching (case-insensitive)
- [ ] Owner/application name matching
- [ ] Bundle ID matching
- [ ] Fuzzy matching as fallback
- [ ] Tests for matching strategies

#### Phase 4: ScreenCaptureKit Capture

- [ ] Initialize ScreenCaptureKit session (macOS 13+)
- [ ] Capture content from window
- [ ] Convert to ImageBuffer
- [ ] Region cropping
- [ ] Scale transformation
- [ ] Cursor inclusion support
- [ ] Async wrapper for sync SCKit API
- [ ] Timeout protection

#### Phase 5: Fallback & Compatibility

- [ ] Fallback to `CGWindowListCreateImage` for macOS 12
- [ ] TCC (Transparency, Consent, Control) handling
- [ ] Permission denied detection with Settings link
- [ ] Apple Silicon (ARM64) optimization

#### Phase 6: Testing & Documentation

- [ ] 15+ unit tests (automated by coding agent)
- [ ] 4+ integration tests (automated by coding agent)
- [ ] Performance on Intel and Apple Silicon (automated by coding agent)
- [ ] TCC permission handling guide
- [ ] M5 completion checklist
- [ ] macOS version compatibility matrix

**Success Criteria:**
- ✅ `list_windows()` returns macOS windows accurately
- ✅ `capture_window()` uses ScreenCaptureKit <2s
- ✅ TCC denied error with Settings link
- ✅ Fallback to CGWindowList for compatibility
- ✅ Cursor support on macOS 13+
- ✅ 50+ unit tests passing
- ✅ Apple Silicon verified
- ✅ macOS 12-14 tested

---

### M6a: Documentation, CI/CD, and Release - Linux and Windows

**Target:** Q1 2026
**Estimated Effort:** 3-5 days
**Dependencies:** M0-M4 complete

**Scope:**

#### Phase 1: User Documentation

- [ ] Comprehensive README (quick start, features)
- [ ] Platform-specific setup guides (Linux/Windows/macOS)
- [ ] User guide (workflow examples)
- [ ] Troubleshooting FAQ (>20 common issues)
- [ ] Performance tuning guide
- [ ] Configuration reference

#### Phase 2: API Documentation

- [ ] Auto-generated rustdoc (cargo doc)
- [ ] Tool schemas with JSON examples
- [ ] Error reference with remediation
- [ ] Architecture overview
- [ ] Backend-specific guides

#### Phase 3: CI/CD Pipeline

- [ ] GitHub Actions workflow (matrix builds)
- [ ] Ubuntu 22.04 + Fedora 39 (X11/Wayland)
- [ ] Windows Server 2022
- [ ] macOS 13+ (deferred until M5)
- [ ] Unit tests in CI (automated by coding agent)
- [ ] Code coverage reporting (automated by coding agent)
- [ ] Linting and formatting checks (automated by coding agent)

#### Phase 4: Release Workflow

- [ ] Automated release on tags (v*)
- [ ] Binary artifacts for all platforms
- [ ] Checksums and signatures
- [ ] GitHub release notes (auto-generated)
- [ ] Changelog management

#### Phase 5: Packaging & Distribution

- [ ] Linux package roadmap (deb, rpm, AUR, Nix)
- [ ] macOS packaging (universal binary, codesigning) (deferred until M5)
- [ ] Windows installer (MSI, Chocolatey)
- [ ] Homebrew formula
- [ ] Quick install script

#### Phase 6: Final QA & Polish

- [ ] End-to-end testing on all platforms (automated by coding agent)
- [ ] Performance regression testing (automated by coding agent)
- [ ] Security review
- [ ] Accessibility review
- [ ] License and attribution review

**Deliverables:**
- ✅ Production-ready documentation
- ✅ Working CI/CD pipeline
- ✅ Automated releases
- ✅ Multi-platform binaries
- ✅ Community communication plan

---

### M6b: macOS CI/CD, Packaging, and Release

**Target:** After M5
**Estimated Effort:** 1-2 days
**Dependencies:** M5 complete

**Scope:**

- [ ] Enable macOS 13+ CI job (unit tests + clippy + fmt)
- [ ] Build and publish macOS artifacts (universal binary if feasible)
- [ ] Codesigning and notarization plan
- [ ] macOS packaging (Homebrew formula / pkg / dmg decision)
- [ ] TCC permissions doc checks in release checklist

**Success Criteria:**
- ✅ macOS CI green on PRs and main
- ✅ Tagged releases include macOS artifacts
- ✅ Clear macOS install and permissions guidance

---

## Cross-Cutting Tasks

### Code Quality

- [ ] Maintain 100% test pass rate (automated by coding agent)
- [ ] Keep clippy warnings at 0 (automated by coding agent)
- [ ] Ensure rustfmt compliance (automated by coding agent)
- [ ] Keep unsafe code minimal
- [ ] Document all public APIs

### Performance

- [ ] Monitor capture latency (target: <2s P95)
- [ ] Track memory usage (<200MB peak)
- [ ] Profile hotspots
- [ ] Optimize image encoding
- [ ] Benchmark transformations

### Testing

- [ ] Add integration tests for each platform (automated by coding agent)
- [ ] Implement image validation (5-layer) (automated by coding agent)
- [ ] Test error paths comprehensively (automated by coding agent)
- [ ] Verify timeout protection (automated by coding agent)
- [ ] Test on edge-case hardware (automated by coding agent where possible)

### Documentation

- [ ] Keep README current
- [ ] Maintain architecture docs
- [ ] Document known limitations
- [ ] Create troubleshooting guides
- [ ] Update changelog

---

## Known Limitations

### Wayland (M2)

- No window enumeration (security model limitation)
- Restore tokens required for headless capture
- Compositor-dependent availability
- Portal UI varies by compositor

### X11 (M3)

- No cursor capture (xcap limitation)
- No per-window alpha channel (EWMH limitation)
- No hardware acceleration (software-based)
- No multi-display indexing (future enhancement)

### Not Yet Implemented

- Video capture (v2.0)
- OCR/text extraction (v2.0)
- Interactive region selection UI (v2.0)
- Linux distro packaging (post-v1.0)

---

## Metrics & Targets

### Quality Gates

- [ ] 100% test pass rate (target: 500+ tests by M6) (automated by coding agent)
- [ ] 0 clippy warnings (automated by coding agent)
- [ ] 100% public API documentation
- [ ] <5 unsafe blocks (platform bindings only)

### Performance Targets

- [ ] Capture latency P95: ≤1.5s
- [ ] List windows: <500ms
- [ ] Memory peak: <200MB
- [ ] Binary size: <20MB

### Test Coverage Targets

- [ ] Unit tests: >80% of codebase (automated by coding agent)
- [ ] Integration tests: All major flows (automated by coding agent)
- [ ] Platform coverage: 4/4 (after M5) (automated by coding agent)
- [ ] Error paths: 100% (automated by coding agent)

---

## Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| Windows GC API instability | Medium | High | Version checking, error recovery |
| macOS TCC permission issues | High | Low | Clear Settings link, retry logic |
| Multi-display edge cases | Medium | Medium | Document limitations, test coverage |
| Compositor-specific quirks | High | Medium | Per-compositor documentation |
| Performance regression | Low | High | Continuous benchmarking |
| Security vulnerability | Low | Critical | Regular audits, dependency updates |

---

## Communication

### Code Review Process

1. All changes via pull requests
2. Automated checks (clippy, fmt, tests) - executed automatically by coding agent
3. Human review for architecture changes
4. Merge on green

### Documentation Updates

- Keep README in sync with code
- Update CHANGELOG.md for releases
- Maintain architecture docs
- Document breaking changes

## Next Steps

1. **Completed:** M4 Windows Backend ✅
   - [x] Add edge case error handling (closed windows, permission denied)
   - [x] Add Windows build version checking (17134+)
   - [x] Increase unit test coverage to 50+ (now 73)
   - [x] Write Windows-specific architecture documentation (WINDOWS_ARCHITECTURE.md)
   - [x] Add integration tests for window/display capture with pixel validation (21 tests)
   - [x] Refactor test infrastructure with shared helpers (`tests/common/windows_helpers.rs`)
   - [x] Add `WindowsTestContext` fixture for cleaner test setup
   - [x] Visual verification tests with AI screenshot analysis

2. **Immediate:** M6a (Linux + Windows docs, CI, and release)
   - Expand CI to include Windows (and keep Linux green)
   - Tighten README quick start + troubleshooting
   - Define release artifacts + checksums for Linux/Windows

3. **Deferred:** M5 macOS backend
   - Research ScreenCaptureKit API and TCC flows
   - Create macOS test plan
   - Implement backend when macOS env is ready

4. **After M5:** M6b macOS CI + packaging

---

**Document Version:** 2.2
**Last Updated:** 2025-12-13
**Next Review:** When M6a complete
