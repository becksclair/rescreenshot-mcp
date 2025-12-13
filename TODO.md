# screenshot-mcp Development Roadmap

## Project Status

**Current:** M0-M3 Complete ✅ | M4-M6 Planned
**Code Quality:** 433 tests passing, 0 warnings, production-ready
**Last Updated:** 2025-11-29

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

---

## Planned Milestones 📅

### M4: Windows Graphics Capture Backend (Next)

**Target:** Q1 2026
**Estimated Effort:** 5-6 days
**Dependencies:** M0-M1 complete

**Scope:**

#### Phase 1: WindowsBackend Module Skeleton

- [ ] Create `src/capture/windows_backend.rs`
- [ ] Define `WindowsBackend` struct with connection management
- [ ] Implement `CaptureFacade` trait with stubs
- [ ] Add feature gate (`windows-graphics-capture`)
- [ ] Export from `src/capture/mod.rs`
- [ ] Basic unit tests (new, capabilities, error stubs)

#### Phase 2: Window Enumeration

- [ ] Implement `EnumWindows` wrapper
- [ ] Query window title, class, executable, PID via Win32 API
- [ ] Filter out system windows (Shell, hidden)
- [ ] Implement `list_windows()` with timeout protection
- [ ] Error handling for access denied
- [ ] Tests for enumeration and filtering

#### Phase 3: Window Resolution

- [ ] Implement `resolve_target()` with matching strategies
- [ ] Title substring matching (case-insensitive)
- [ ] Class matching (exact or partial)
- [ ] Executable path matching
- [ ] Fuzzy matching as fallback
- [ ] Tests for all matching strategies

#### Phase 4: Capture Implementation

- [ ] Integrate Windows.Graphics.Capture API
- [ ] Initialize capture session from window handle
- [ ] Frame acquisition and conversion to ImageBuffer
- [ ] Region cropping support
- [ ] Scale transformation support
- [ ] Cursor inclusion via WGC flags
- [ ] Async-safe spawn_blocking wrapper
- [ ] Timeout protection (2s default)
- [ ] Tests and integration tests

#### Phase 5: Error Handling & Edge Cases

- [ ] Map Windows API errors to CaptureError
- [ ] Handle closed windows gracefully
- [ ] Permission denied detection
- [ ] WGC unavailable detection
- [ ] Build version checking (17134+)
- [ ] Comprehensive error messages with remediation
- [ ] Tests for all error paths

#### Phase 6: Testing & Documentation

- [ ] 15+ unit tests covering all paths (automated by coding agent)
- [ ] 4+ integration tests (automated by coding agent)
- [ ] Performance benchmarking (automated by coding agent)
- [ ] Windows-specific architecture documentation
- [ ] M4 completion checklist
- [ ] Known limitations documentation

**Success Criteria:**
- ✅ `list_windows()` returns accurate data
- ✅ `capture_window()` captures by title/class/exe <2s
- ✅ Cursor included when `include_cursor: true`
- ✅ Error handling for WGC unavailable
- ✅ 50+ unit tests passing
- ✅ Zero warnings, fully formatted
- ✅ Windows 10/11 compatibility verified

---

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

### M6: Documentation, CI/CD, and Release

**Target:** Q1 2026
**Estimated Effort:** 3-5 days
**Dependencies:** M0-M5 complete

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
- [ ] macOS 13+
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
- [ ] macOS packaging (universal binary, codesigning)
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

1. **Immediate:** Keep M0-M3 stable
   - Monitor test pass rate
   - Fix any emerging issues
   - Maintain documentation

2. **Short-term:** Prepare for M4
   - Research Windows Graphics Capture API
   - Set up Windows development environment
   - Create Windows test plan

3. **Medium-term:** Implement M4-M5
   - Windows backend (4-6 days)
   - macOS backend (5-7 days)
   - Parallel execution if resources available

4. **Long-term:** M6 Release
   - Finalize documentation
   - Configure CI/CD
   - Package and distribute
   - Launch and support

---

**Document Version:** 2.0
**Last Updated:** 2025-11-29
**Next Review:** When M4 begins
