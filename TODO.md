# screenshot-mcp Development Roadmap

## Project Status

**Current:** M0-M4, M6a Complete ✅ | M5 Deferred
**Code Quality:** 500+ tests passing, 0 warnings, production-ready
**Last Updated:** 2025-12-14

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

### M6a: Documentation, CI/CD, and Release (Complete)

- [x] Comprehensive README (quick start, features)
- [x] Platform-specific setup guides (Linux/Windows)
- [x] User guide (workflow examples)
- [x] Troubleshooting FAQ (>20 common issues)
- [x] Performance tuning guide
- [x] Auto-generated rustdoc (cargo doc)
- [x] Tool schemas with JSON examples
- [x] GitHub Actions workflow (matrix builds: Ubuntu, Fedora, Windows)
- [x] Code coverage reporting
- [x] Automated release workflow on tags
- [x] Binary artifacts with checksums
- [x] Packaging roadmap
- [x] Install scripts (sh/ps1)

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

### Architectural Improvements (Post-M6)

**Target:** When bandwidth allows
**Priority:** Nice-to-have (not blocking releases)
**Added:** 2025-12-15 (from code review retrospective)

#### A1: Capability-Based Backend Traits

**Problem:** The current `CaptureFacade` trait is a "mega-trait" that forces all backends to implement methods they can't meaningfully support. For example, Wayland cannot enumerate windows (security model limitation), but it must implement `list_windows()` which returns fake "primed source" entries.

**Current Design:**
```rust
#[async_trait]
pub trait CaptureFacade: Send + Sync {
    async fn list_windows(&self) -> CaptureResult<Vec<WindowInfo>>;
    async fn resolve_target(&self, selector: &WindowSelector) -> CaptureResult<WindowHandle>;
    async fn capture_window(&self, handle: WindowHandle, opts: &CaptureOptions) -> CaptureResult<ImageBuffer>;
    async fn capture_display(&self, display_id: Option<u32>, opts: &CaptureOptions) -> CaptureResult<ImageBuffer>;
    fn capabilities(&self) -> Capabilities;
    fn as_any(&self) -> &dyn std::any::Any;
}
```

**Proposed Design:**
```rust
// Composable capability traits
#[async_trait]
pub trait WindowEnumerator: Send + Sync {
    async fn list_windows(&self) -> CaptureResult<Vec<WindowInfo>>;
}

#[async_trait]
pub trait WindowResolver: Send + Sync {
    async fn resolve(&self, selector: &WindowSelector) -> CaptureResult<WindowHandle>;
}

#[async_trait]
pub trait ScreenCapture: Send + Sync {
    async fn capture(&self, target: CaptureTarget, opts: &CaptureOptions) -> CaptureResult<ImageBuffer>;
}

// Backends implement only what they support
struct WaylandBackend;  // implements ScreenCapture only
struct X11Backend;      // implements WindowEnumerator + WindowResolver + ScreenCapture
struct WindowsBackend;  // implements WindowEnumerator + WindowResolver + ScreenCapture
```

**Implementation Steps:**

1. **Phase 1: Define new traits** (non-breaking)
   - [ ] Create `WindowEnumerator`, `WindowResolver`, `ScreenCapture` traits in `capture/traits.rs`
   - [ ] Add blanket impls so existing `CaptureFacade` implementors auto-implement new traits
   - [ ] Add tests for trait composition

2. **Phase 2: Update backends** (non-breaking)
   - [ ] Add explicit trait impls to `WindowsBackend`, `X11Backend`, `WaylandBackend`
   - [ ] Remove fake `list_windows()` from `WaylandBackend` (return error instead)
   - [ ] Update `LinuxAutoBackend` to check capabilities before delegating

3. **Phase 3: Update MCP handlers** (breaking)
   - [ ] Update `list_windows` tool to check if backend implements `WindowEnumerator`
   - [ ] Return appropriate error for Wayland-only sessions
   - [ ] Update tool schemas to document capability requirements

4. **Phase 4: Deprecate `CaptureFacade`**
   - [ ] Mark `CaptureFacade` as `#[deprecated]`
   - [ ] Update all consumers to use capability traits
   - [ ] Remove in next major version

**Files to Modify:**
- `crates/screenshot-core/src/capture/mod.rs` (new traits)
- `crates/screenshot-core/src/capture/wayland_backend.rs`
- `crates/screenshot-core/src/capture/x11_backend.rs`
- `crates/screenshot-core/src/capture/windows_backend.rs`
- `crates/screenshot-mcp-server/src/mcp.rs`

**Breaking Changes:** Yes (Phase 3+)
**Estimated Effort:** 2-3 days

---

#### A2: MCP Streaming Support for Large Images

**Problem:** Large screenshots (4K displays, multi-monitor captures) can produce 10+ MB of encoded image data. The current implementation loads the entire image into memory and returns it as a single base64 blob, which can cause:
- Memory pressure on both server and client
- Timeout issues for slow connections
- Poor UX when waiting for large transfers

**Current Design:**
```rust
// capture_window returns entire image in memory
let image = backend.capture_window(handle, &opts).await?;
let encoded = encode_image(&image, format, quality)?;
let base64 = base64::encode(&encoded);
Ok(CallToolResult::success(vec![Content::image(base64, mime_type)]))
```

**Proposed Design:**

1. **File-based response for large images:**
```rust
// For images > 1MB, write to temp file and return path
if encoded.len() > 1_048_576 {
    let temp_path = temp_files.write_image(&encoded, format)?;
    Ok(CallToolResult::success(vec![
        Content::resource(ResourceLink {
            uri: format!("file://{}", temp_path.display()),
            mime_type: Some(mime_type.to_string()),
            ..Default::default()
        })
    ]))
} else {
    // Small images: inline base64
    Ok(CallToolResult::success(vec![Content::image(base64, mime_type)]))
}
```

2. **Progressive JPEG option:**
```rust
// Add progressive encoding for bandwidth-constrained scenarios
pub struct CaptureOptions {
    // ... existing fields
    pub progressive: bool,  // Use progressive JPEG encoding
}
```

3. **Capture-to-file tool:**
```rust
#[tool(description = "Capture window and save to file (no base64 overhead)")]
pub async fn capture_window_to_file(
    &self,
    params: CaptureWindowToFileParams,
) -> Result<CallToolResult, McpError> {
    // Returns only file path, not image data
}
```

**Implementation Steps:**

1. **Phase 1: Add file-based fallback**
   - [ ] Add size threshold constant (`INLINE_IMAGE_MAX_BYTES = 1_048_576`)
   - [ ] Update `build_capture_result()` to check encoded size
   - [ ] Return `ResourceLink` for large images
   - [ ] Add temp file cleanup on session end

2. **Phase 2: Add capture_to_file tool**
   - [ ] Create `CaptureWindowToFileParams` struct
   - [ ] Implement `capture_window_to_file` tool handler
   - [ ] Return structured response with file path and metadata
   - [ ] Add tool to schema

3. **Phase 3: Progressive encoding**
   - [ ] Add `progressive` field to `CaptureOptions`
   - [ ] Update JPEG encoder to support progressive mode
   - [ ] Add tests for progressive encoding
   - [ ] Document tradeoffs (faster perceived load, slightly larger file)

4. **Phase 4: MCP streaming (if rmcp supports it)**
   - [ ] Investigate rmcp streaming capabilities
   - [ ] Implement chunked transfer if available
   - [ ] Add progress reporting for large captures

**Files to Modify:**
- `crates/screenshot-mcp-server/src/mcp_content.rs` (size-based routing)
- `crates/screenshot-mcp-server/src/mcp.rs` (new tool)
- `crates/screenshot-core/src/model.rs` (progressive option)
- `crates/screenshot-core/src/util/encode.rs` (progressive JPEG)

**Breaking Changes:** No (additive)
**Estimated Effort:** 2-3 days

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

### Windows (M4)

- **Test exit code 0xe06d7363**: Windows C++ exception during test cleanup. All 297 tests pass but the test harness process crashes on teardown with exit code `0xe06d7363` (Windows SEH exception). This appears to be a cleanup/teardown issue in the Windows Graphics Capture API or FFI layer, not a functional bug. Needs further investigation - likely related to COM object release order or WGC frame pool disposal.

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

1. **Completed:** M6a (Linux + Windows docs, CI, and release) ✅
   - [x] Docs: User/API/Perf/Troubleshooting
   - [x] CI: Matrix (Ubuntu/Fedora/Windows) + Coverage
   - [x] Release: Automation + Checksums
   - [x] Install scripts

2. **Completed:** Code quality improvements from retrospective ✅
   - [x] Feature-gated integration tests (CI stability)
   - [x] Global LRU regex cache (memory efficiency)
   - [x] Environment variable timeout overrides (configurability)
   - [x] XDG_STATE_HOME for machine keys (XDG compliance)
   - [x] Structured error hints (LLM auto-recovery)
   - [x] Tracing instrumentation (observability)

3. **Immediate:** M5 macOS backend
   - Research ScreenCaptureKit API and TCC flows
   - Create macOS test plan
   - Implement backend when macOS env is ready

4. **Deferred:** M6b macOS CI + packaging

5. **Future:** Architectural improvements (A1, A2)
   - A1: Capability-based backend traits (cleaner abstraction)
   - A2: MCP streaming for large images (performance)

---

**Document Version:** 3.1 (Code Quality Improvements)
**Last Updated:** 2025-12-15
**Next Review:** When M5 complete
