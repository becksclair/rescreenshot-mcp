# screenshot-mcp: Cross-Platform Screenshot MCP Server

## Complete Feature Specification v1.0

**Generated:** 2024-10-13
**Status:** Approved
**Timeline:** 6 weeks (M0-M6)

---

## Table of Contents

1. [Project Overview](#project-overview)
2. [Clarifications & Decisions](#clarifications--decisions)
3. [Milestone Breakdown](#milestone-breakdown)
4. [Product Management](#product-management)
5. [Product Design](#product-design)
6. [Software Architecture](#software-architecture)
7. [Quality Assurance](#quality-assurance)
8. [Traceability & Compliance](#traceability--compliance)
9. [Next Steps](#next-steps)

---

## Project Overview

**screenshot-mcp** is a production-grade, cross-platform Model Context Protocol (MCP) stdio server that enables coding agents (Claude, Cursor, etc.) to capture application windows and return screenshots programmatically across Wayland/X11 Linux, Windows 10/11, and macOS 12+. The server addresses the critical need for headless, permission-persistent window capture—particularly on Wayland where XDG Desktop Portal restore tokens eliminate repetitive user prompts after initial consent.

### Key Features

- **4 MCP Tools:** `list_windows`, `capture_window`, `prime_wayland_consent`, `health_check`
- **Dual-format output:** Inline PNG image blocks + timestamped `file://` ResourceLinks
- **Platform backends:**
  - Wayland: XDG Desktop Portal with keyring-backed restore tokens
  - X11: x11rb enumeration + xcap capture
  - Windows: Windows Graphics Capture (WGC) with cursor support
  - macOS: ScreenCaptureKit (12+) with TCC permission handling
- **Image formats:** PNG (default), WebP, JPEG with configurable quality/scale
- **Performance:** <2s capture latency (P95 target: ≤1.5s)

### Scope Boundaries

**In Scope (v1.0):**
- All 4 platforms with complete feature set
- Window enumeration and capture by title/class/exe selector
- Wayland restore token persistence (headless after first consent)
- Comprehensive documentation (README, User Guide, Troubleshooting FAQ, API docs)
- CI/CD with matrix builds (Ubuntu, Fedora, Windows, macOS)

**Out of Scope (v1.0, deferred to v2):**
- Video capture
- OCR (text extraction from screenshots)
- Interactive region selection UI
- Multi-monitor enumeration beyond basic display IDs
- Linux distribution packaging (roadmap only; implementation post-v1.0)

### Success Metrics (90-day post-launch)

| Metric | Target | Measurement |
|--------|--------|-------------|
| **Adoption** | ≥500 GitHub stars | GitHub API |
| **Wayland Headless Success** | ≥95% | User-reported telemetry (opt-in) |
| **Capture Latency (P95)** | ≤1.5s | Tracing logs (local) |
| **Support Load** | <10 issues/week | GitHub Issues |
| **Platform Coverage** | 4/4 platforms functional | CI test pass rate |

---

## Clarifications & Decisions

### Key Decisions (from Q&A)

1. **Success metric:** Works properly on all target platforms
2. **Timeline:** Full v1.0 includes all platforms; one-pass = scaffold + basic MCP server, rest iterative
3. **Users:** Coding agents only (no CLI users for v1.0)
4. **Linux distros:** Ubuntu, Fedora, Arch, NixOS (packaging last, after all features)
5. **Windows:** 10 and 11 support
6. **macOS:** 12+ (ScreenCaptureKit)
7. **v1 scope:** Full feature set (4 tools, all 4 platforms, restore tokens, ResourceLinks)
8. **Performance:** <2s for capture
9. **Keyring:** Platform keyring, only if necessary
10. **Temp files:** Persist until process exit
11. **MCP SDK:** `rmcp` (official Rust MCP SDK)
12. **Wayland token reliability:** <5% failure rate acceptable
13. **Error recovery:** Quick retry, then fail fast
14. **Observability:** Tracing logs only (no metrics/telemetry)
15. **Security scanning:** No (for v1.0)
16. **Release:** Simple v1.0
17. **CI/CD:** Yes, GitHub Actions with matrix builds + artifacts
18. **Documentation:** README + API docs + user guide + troubleshooting FAQ
19. **Wayland testing:** KDE Plasma (user's env), assume portal compliance for others
20. **Wayland fallback:** Display capture + region crop
21. **Resource links:** Timestamped (unique per capture)
22. **License:** MIT recommended (pending user final decision)

---

## Milestone Breakdown

### Dependency Graph

```
M0 (scaffold) → M1 (core facade)
                  ↓
        ┌─────────┼─────────┬─────────┐
        ↓         ↓         ↓         ↓
       M2        M3        M4        M5
    (Wayland)  (X11)   (Windows)  (macOS)
        └─────────┴─────────┴─────────┘
                      ↓
                     M6
            (docs/CI/packaging)
```

### Priority & Status Legend

- **Priority:** 🔴 P0 (Blocker) · 🟠 P1 (High) · 🟡 P2 (Medium) · 🔵 P3 (Low) · ⚪ P4 (Backlog)
- **Status:** ⏳ Not started · 🚧 In progress · 🤖 Verifying/QA · ✅ Done · ⛔ Blocked · 🕒 Waiting/External

---

### 🔧 M0: Project Scaffold & Basic MCP Server

**Priority:** 🔴 P0 (Blocker)
**Timeline:** Week 1
**Status:** ⏳ Not started

**Objective:** Establish project structure, dependencies, and minimal stdio MCP server with `health_check` tool to validate protocol integration.

**Scope:**
- Initialize Cargo workspace with project layout
- Configure dependencies (`rmcp`, `serde`, `schemars`, `thiserror`, `tracing`, `tempfile`, `image`)
- Implement stdio MCP server with `health_check` tool
- Add platform detection logic (Linux: Wayland vs X11, Windows, macOS)
- Write unit tests for model serialization and platform detection
- Configure linting (clippy, rustfmt)

**Exit Criteria:**
- ✅ `cargo build --all-features` succeeds on Linux
- ✅ `cargo test` passes all unit tests
- ✅ MCP server starts and responds to `health_check` tool call via stdio
- ✅ `clippy --all-targets --all-features -D warnings` clean
- ✅ Code formatted with `rustfmt`

**Acceptance Tests:**
- **T-M0-01:** Start server, send MCP `initialize` → receive valid response
- **T-M0-02:** Call `health_check` → receive `{ "platform": "linux", "backend": "none", "ok": true }`
- **T-M0-03:** Platform detection correctly identifies Wayland/X11

**Dependencies:** Rust 1.75+ toolchain, `rmcp` crate access

**Risks:**
- **Risk:** `rmcp` crate immature/undocumented
- **Mitigation:** Allocate 1 day to prototype; fallback to manual JSON-RPC

---

### 🏗️ M1: Core Capture Facade & Image Handling

**Priority:** 🔴 P0 (Blocker)
**Timeline:** Week 2
**Status:** ⏳ Not started

**Objective:** Design and implement `CaptureFacade` trait with platform backend registration, image encoding pipeline, and temp file ResourceLink generation.

**Scope:**
- Define `CaptureFacade` trait with methods: `resolve_target()`, `capture_window()`, `capture_display()`, `capture_region()`
- Implement `MockBackend` for testing
- Create `ImageBuffer` wrapper with scale/crop utilities
- Implement encoding pipeline (PNG/WebP/JPEG with quality param)
- Build MCP content builders: `build_image_content()` and `build_resource_link()`
- Add tempfile management with cleanup on process exit
- Update tool stubs (`list_windows`, `capture_window`) with mock data
- Define error types (`CaptureError` enum)

**Exit Criteria:**
- ✅ `MockBackend` generates 1920x1080 test image, returns MCP result <2s
- ✅ PNG/WebP/JPEG encoding tests pass with quality validation
- ✅ Temp files persist across captures, cleanup on exit
- ✅ All error types documented with user-facing messages

**Acceptance Tests:**
- **T-M1-01:** `capture_window` with `MockBackend` → PNG image + ResourceLink with correct MIME
- **T-M1-02:** Encode 1920x1080 as WebP quality=80 → <200KB, visually acceptable
- **T-M1-03:** Capture 3 screenshots → verify 3 unique temp files with timestamps
- **T-M1-04:** Process exits → temp files cleaned up

**Dependencies:** M0 complete, `image` crate with WebP/JPEG features

**Risks:**
- **Risk:** Image encoding slow (>500ms for 4K)
- **Mitigation:** Profile and optimize; use rayon if needed

---

### 🌊 M2: Wayland Backend with Restore Tokens

**Priority:** 🔴 P0 (Blocker)
**Timeline:** Week 3
**Status:** ⏳ Not started

**Objective:** Implement Wayland XDG Desktop Portal Screencast backend with headless restore token persistence via platform keyring, tested on KDE Plasma.

**Scope:**
- Implement `WaylandBackend` using `ashpd::desktop::screencast`
- Create `KeyStore` wrapper around `keyring` crate
- Implement `prime_wayland_consent` tool
- Update `list_windows` (return mock entry with note)
- Update `capture_window`: restore with token → capture → rotate token; fallback to display+crop
- Handle `PersistMode::ExplicitlyRevoked`
- Add timeout handling (5s) with retry logic
- Write integration tests (manual/gated)

**Exit Criteria:**
- ✅ `prime_wayland_consent` opens KDE portal, stores token
- ✅ Second `capture_window` succeeds headlessly
- ✅ Token rotation: third call uses newly rotated token
- ✅ Fallback: restore fails → display capture + region crop succeeds
- ✅ Error messages actionable

**Acceptance Tests:**
- **T-M2-01:** Fresh install → prime consent → token stored
- **T-M2-02:** Restart → capture headlessly <2s
- **T-M2-03:** Simulate compositor restart → re-prompt, store new token
- **T-M2-04:** Restore fails, display+region → cropped capture succeeds
- **T-M2-05:** Keyring unavailable → fallback to file, warning logged

**Dependencies:** M1 complete, KDE Plasma test environment

**Risks:**
- **Risk:** KDE Plasma portal bugs
- **Mitigation:** Test on Plasma 5.27+, document known issues
- **Risk:** Token revocation >5%
- **Mitigation:** Robust fallback, log reasons

---

### 🖼️ M3: X11 Backend

**Priority:** 🟠 P1 (High)
**Timeline:** Week 3-4
**Status:** ⏳ Not started

**Objective:** Implement X11 window enumeration and capture using `x11rb` and `xcap`.

**Scope:**
- Implement `X11Backend` with connection management
- `list_windows`: Query `_NET_CLIENT_LIST`, extract properties
- `resolve_target`: Match selector (title/class/PID) with fuzzy matching
- `capture_window`: Use `xcap` by window ID
- Add display detection (`$DISPLAY`)
- Write integration tests (gated, Xvfb)

**Exit Criteria:**
- ✅ `list_windows` returns all windows with correct metadata
- ✅ `capture_window` with title selector matches and captures <2s
- ✅ Regex matching works
- ✅ Window closed mid-capture → `WindowNotFound` error

**Acceptance Tests:**
- **T-M3-01:** Start xterm on Xvfb → `list_windows` includes xterm
- **T-M3-02:** Capture xterm by class → returns screenshot
- **T-M3-03:** Fuzzy match "term" → matches xterm
- **T-M3-04:** Window closed → `WindowNotFound` with window ID

**Dependencies:** M1 complete, X11 test environment

---

### 🪟 M4: Windows Backend

**Priority:** 🟠 P1 (High)
**Timeline:** Week 4-5
**Status:** ⏳ Not started

**Objective:** Implement Windows enumeration via `EnumWindows` and capture using Windows Graphics Capture API.

**Scope:**
- Implement `WindowsBackend`
- `list_windows`: Use `EnumWindows`, collect HWND/title/class/PID/exe
- `resolve_target`: Match by title/class/exe
- `capture_window`: Initialize WGC, capture frame
- Handle cursor inclusion via WGC flags
- Add error handling for WGC unavailable
- Write integration tests (gated)

**Exit Criteria:**
- ✅ `list_windows` on Win11 returns all windows
- ✅ `capture_window` with exe selector captures Notepad <2s
- ✅ `includeCursor: true` includes cursor
- ✅ WGC unavailable → error with upgrade message

**Acceptance Tests:**
- **T-M4-01:** Start Notepad → `list_windows` includes Notepad
- **T-M4-02:** Capture by title → returns screenshot
- **T-M4-03:** Cursor visible with `includeCursor: true`
- **T-M4-04:** Win10 <17134 → error with build number

**Dependencies:** M1 complete, Win10/11 test VM

---

### 🍎 M5: macOS Backend

**Priority:** 🟠 P1 (High)
**Timeline:** Week 5
**Status:** ⏳ Not started

**Objective:** Implement macOS enumeration via `CGWindowListCopyWindowInfo` and capture using ScreenCaptureKit.

**Scope:**
- Implement `MacBackend`
- `list_windows`: Call `CGWindowListCopyWindowInfo`
- `resolve_target`: Match by title/owner
- `capture_window`: ScreenCaptureKit (macOS 12+), fallback to `CGWindowListCreateImage`
- Handle TCC permission checks
- Add cursor inclusion (SCKit only)
- Write integration tests (gated)

**Exit Criteria:**
- ✅ `list_windows` on macOS 14 returns all windows
- ✅ `capture_window` uses ScreenCaptureKit <2s
- ✅ TCC denied → error with Settings deep link
- ✅ Fallback to CGWindowList if needed

**Acceptance Tests:**
- **T-M5-01:** Fresh install → TCC prompt → grant → `health_check` passes
- **T-M5-02:** Denied → error with Settings path
- **T-M5-03:** Safari open → `list_windows` includes Safari
- **T-M5-04:** Capture Safari via SCKit

**Dependencies:** M1 complete, macOS 12+ test machine

---

### 📚 M6: Documentation, CI/CD, and Packaging

**Priority:** 🟡 P2 (Medium)
**Timeline:** Week 6
**Status:** ⏳ Not started

**Objective:** Complete documentation, configure CI/CD, prepare packaging roadmap.

**Scope:**
- Write README: Quick Start, Features, MCP config snippets
- Generate API documentation (JSON Schema)
- Create User Guide and Troubleshooting FAQ
- Configure GitHub Actions CI: matrix builds
- Add release workflow: tagged releases → artifacts
- Document packaging roadmap (defer implementation)
- Add LICENSE file
- Create examples directory
- Final QA on all platforms

**Exit Criteria:**
- ✅ README Quick Start works copy-paste on all platforms
- ✅ API docs auto-generated, include all tools
- ✅ User guide covers first-run for Wayland and macOS
- ✅ FAQ has ≥10 common issues
- ✅ CI passes on all platforms
- ✅ Release workflow produces binaries
- ✅ Packaging roadmap documented

**Acceptance Tests:**
- **T-M6-01:** New user follows README → MCP server works
- **T-M6-02:** "Portal unavailable" → finds solution in FAQ
- **T-M6-03:** CI matrix build → all platforms pass
- **T-M6-04:** Tag v1.0.0 → binaries created
- **T-M6-05:** macOS TCC setup → captures window

**Dependencies:** M0-M5 complete

---

## Product Management

### Business Case

**Problem:** Coding agents need programmatic screenshot access for UI inspection, bug analysis, and automation. Existing solutions lack cross-platform consistency, MCP integration, and—on Wayland—require repetitive user consent.

**Strategic Value:**
- **Developer Experience:** "Show me what you see" workflows
- **Platform Parity:** Unified API across Linux/Windows/macOS
- **Wayland Leadership:** First mover with headless restore tokens

### User Stories (INVEST Format)

**Epic 1: Core Capture (P0)**
- **US-1.1:** As a coding agent, I want to list windows to identify targets
  - **AC:** `list_windows` returns metadata within 200ms
- **US-1.2:** As a coding agent, I want to capture by title to inspect UI
  - **AC:** `capture_window` returns PNG + ResourceLink <2s
- **US-1.3:** As a coding agent, I want dual-format output for client flexibility
  - **AC:** Result includes image block + ResourceLink

**Epic 2: Wayland Headless (P0)**
- **US-2.1:** As a Wayland user, I want permission to persist after first grant
  - **AC:** After prime, next capture is headless
- **US-2.2:** As a developer, I want automatic token rotation
  - **AC:** Each capture rotates token

**Epic 3: Cross-Platform (P1)**
- **US-3.1:** As an X11 user, I want enumeration/capture equivalent to Wayland
  - **AC:** Works with WM_CLASS/title matching
- **US-3.2:** As a Windows user, I want WGC with cursor
  - **AC:** `includeCursor: true` works
- **US-3.3:** As a macOS user, I want ScreenCaptureKit with TCC guidance
  - **AC:** Startup checks TCC, provides Settings link

**Epic 4: Developer Experience (P2)**
- **US-4.1:** As a developer, I want clear API docs
  - **AC:** README includes examples
- **US-4.2:** As a user, I want actionable errors
  - **AC:** Errors include remediation steps

### KPIs & Guardrails

| KPI | Target | Baseline | Measurement | Guardrail |
|-----|--------|----------|-------------|-----------|
| Adoption | 500 stars (90d) | 0 | GitHub API | <50 = reevaluate |
| Wayland Headless | ≥95% | N/A | Telemetry (opt-in) | <80% = investigate |
| Capture Latency | ≤1.5s (P95) | N/A | Tracing logs | >3s = regression |
| Support Load | <10 issues/week | N/A | GitHub Issues | >20 = triage |
| Platform Coverage | 4/4 functional | 0 | CI pass rate | <3/4 = delay launch |

### Rollout Plan

1. **Alpha (Week 1-2):** Internal team testing (scaffold + mock)
2. **Beta (Week 3-5):** Community early adopters (10 users, 3+ platforms)
3. **v1.0 (Week 6):** Public release via GitHub + community channels

**Communication:**
- Pre-launch: GitHub Discussions beta invite
- Launch: Discord #mcp, r/rust, HackerNews (Show HN)
- Post-launch: GitHub Issues for support

---

## Product Design

### Core Flows

**Flow 1: Initial Setup (Wayland)**
1. User installs binary to `~/.local/bin/`
2. User adds MCP config to Claude Desktop
3. Claude launches server via stdio
4. Agent calls `health_check` → backend detected
5. Agent calls `prime_wayland_consent` → picker opens
6. User selects target app → token stored
7. Agent calls `capture_window` → headless capture succeeds

**Flow 2: Routine Capture (Post-Setup)**
1. Agent needs VSCode screenshot
2. Agent calls `list_windows` → receives metadata
3. Agent calls `capture_window` with selector
4. Server captures headlessly, encodes, saves temp file
5. Returns image content + ResourceLink

**Flow 3: Error Recovery (Token Revocation)**
1. User restarts compositor
2. Token invalidated
3. Agent captures → restore fails
4. Server falls back to display capture or re-prompts
5. New token stored, capture succeeds

**Flow 4: X11 Fallback**
1. User on Xorg session
2. `health_check` returns `backend: "x11"`
3. Agent calls `list_windows` → direct enumeration
4. Agent captures → no consent flow needed

### API Response States

**Success:**
```json
{
  "content": [
    { "type": "image", "mimeType": "image/png", "data": "..." },
    { "type": "resource", "resource": {
      "uri": "file:///tmp/screenshot-mcp-1697123456.png",
      "mimeType": "image/png", "size": 245680,
      "title": "Firefox Screenshot - 2024-10-13T14:32:15Z"
    }}
  ],
  "metadata": { ... }
}
```

**Error (Window Not Found):**
```json
{
  "error": {
    "code": "WindowNotFound",
    "message": "No window matching selector: {...}",
    "details": { "hint": "Call list_windows to see targets" }
  }
}
```

**Error (Portal Unavailable):**
```json
{
  "error": {
    "code": "PortalUnavailable",
    "message": "XDG Desktop Portal Screencast not found",
    "details": {
      "remediation": "Install xdg-desktop-portal-kde. Restart session."
    }
  }
}
```

**Error (TCC Permission Denied):**
```json
{
  "error": {
    "code": "PermissionDenied",
    "message": "Screen Recording permission not granted",
    "details": {
      "remediation": "Open System Settings > Privacy & Security > Screen Recording",
      "deepLink": "x-apple.systempreferences:..."
    }
  }
}
```

---

## Software Architecture

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    MCP Client (Claude Desktop)               │
└──────────────────────────┬──────────────────────────────────┘
                           │ JSON-RPC over stdio
┌──────────────────────────▼──────────────────────────────────┐
│                   screenshot-mcp Server                      │
│  ┌────────────┐  ┌────────────┐  ┌──────────────┐          │
│  │   main.rs  │─▶│   mcp.rs   │─▶│   model.rs   │          │
│  │ (stdio I/O)│  │ (routing)  │  │ (types)      │          │
│  └────────────┘  └────────────┘  └──────────────┘          │
│         │              │                                      │
│         ▼              ▼                                      │
│  ┌──────────────────────────────────────────────┐           │
│  │         CaptureFacade (trait)                │           │
│  │  - resolve_target(selector) -> Handle        │           │
│  │  - capture_window(handle, opts) -> Image     │           │
│  └──────────────┬───────────────────────────────┘           │
│                 │                                             │
│      ┌──────────┼──────────┬─────────────┬──────────┐       │
│      ▼          ▼          ▼             ▼          ▼       │
│  ┌───────┐ ┌───────┐ ┌──────────┐ ┌─────────┐ ┌────────┐  │
│  │Wayland│ │  X11  │ │ Windows  │ │  macOS  │ │  Mock  │  │
│  │Backend│ │Backend│ │ Backend  │ │ Backend │ │Backend │  │
│  └───┬───┘ └───┬───┘ └────┬─────┘ └────┬────┘ └────────┘  │
└──────┼─────────┼──────────┼────────────┼───────────────────┘
       │         │          │            │
       ▼         ▼          ▼            ▼
  ┌────────┐ ┌──────┐ ┌──────────┐ ┌──────────┐
  │ ashpd  │ │x11rb │ │windows-  │ │ objc2-   │
  │ (DBus) │ │xcap  │ │capture   │ │screen-   │
  └────────┘ └──────┘ └──────────┘ └──────────┘
```

### Tech Stack

| Component | Technology | Rationale |
|-----------|-----------|-----------|
| MCP SDK | `rmcp` | Official Rust MCP SDK |
| Wayland | `ashpd` | Async DBus Portal bindings |
| X11 | `x11rb` + `xcap` | Window enumeration + capture |
| Windows | `windows-capture` | WGC API wrapper |
| macOS | `objc2-screen-capture-kit` | ScreenCaptureKit bindings |
| Encoding | `image` crate | Unified PNG/WebP/JPEG |
| Keyring | `keyring` | Cross-platform secret storage |
| Logging | `tracing` | Structured, filterable logs |
| Errors | `thiserror` | Typed error handling |

### Data Model

**Core Types:**

```rust
pub struct WindowSelector {
    pub title_substring_or_regex: Option<String>,
    pub class: Option<String>,
    pub exe: Option<String>,
}

pub enum ImageFormat { Png, Webp, Jpeg }

pub struct CaptureOptions {
    pub format: ImageFormat,
    pub quality: Option<u8>, // 0-100
    pub scale: Option<f32>, // 0.1-2.0
    pub include_cursor: bool,
    pub region: Option<Region>,
    pub wayland_source: Option<WaylandSource>,
}

pub struct WindowInfo {
    pub id: String,
    pub title: String,
    pub class: String,
    pub owner: String,
    pub pid: u32,
    pub backend: String,
}
```

**Backend Trait:**

```rust
#[async_trait]
pub trait CaptureFacade: Send + Sync {
    async fn list_windows(&self) -> Result<Vec<WindowInfo>>;
    async fn resolve_target(&self, selector: &WindowSelector) -> Result<WindowHandle>;
    async fn capture_window(&self, handle: WindowHandle, opts: &CaptureOptions) -> Result<ImageBuffer>;
    async fn capture_display(&self, display_id: Option<u32>, opts: &CaptureOptions) -> Result<ImageBuffer>;
    fn capabilities(&self) -> Capabilities;
}
```

### Security & Compliance

**Secrets Management:**
- Wayland restore tokens stored in platform keyring
- Scope: `screenshot-mcp-wayland-<source>`
- Rotation: Single-use tokens, auto-replaced
- Fallback: Encrypted file if keyring unavailable

**Input Validation:**
- Regex injection prevention (safe parsing)
- File paths: temp files only in `~/.cache/screenshot-mcp/`
- Quality/scale parameters: clamped to valid ranges

**Privacy:**
- No screenshot content logged
- Temp files user-owned, unencrypted
- Wayland portal requires explicit user selection

### Operability

**Observability:**
- Structured tracing logs (JSON or console)
- Levels: ERROR, WARN, INFO, DEBUG, TRACE
- Config: `RUST_LOG=screenshot_mcp=info` (default)

**Configuration:**
- `RUST_LOG`: Logging level
- `SCREENSHOT_MCP_TEMP_DIR`: Override temp directory
- `SCREENSHOT_MCP_TIMEOUT_MS`: Capture timeout (default: 2000ms)

**Health Checks:**
- `health_check` tool validates backend availability
- Startup checks: platform detection, portal/TCC permissions

---

## Quality Assurance

### Test Strategy

**Test Pyramid:**
- **Unit Tests (70%):** Model serialization, selector matching, platform detection, encoding
- **Integration Tests (20%):** Backend enumeration/capture, portal flows, keyring (gated)
- **E2E Tests (10%):** Full user flows, manual across platforms

**Unit Test Coverage Target:** ≥80% for core modules (`model.rs`, `util/`, `capture/mod.rs`)

**Integration Tests (Gated):**
- Feature flag: `#[cfg(feature = "integration-tests")]`
- Env var: `RUN_INTEGRATION_TESTS=1`
- Requires GUI/compositor

**Performance Benchmarks:**
- PNG encoding (1920x1080): <300ms
- WebP encoding: <200ms
- Full capture flow: <2s (P95)
- Memory peak: <200MB

### CI/CD Pipeline

**GitHub Actions Matrix:**
- Ubuntu 22.04 (X11/Xvfb only)
- Fedora 39 (Docker, X11/Xvfb)
- Windows Server 2022
- macOS 13

**Pipeline Stages:**
1. Lint & Format (`clippy`, `rustfmt`)
2. Unit Tests (`cargo test --all-features`)
3. Integration Tests (partial, gated)
4. Build Artifacts (release binaries)
5. Performance Regression (benchmarks vs baseline)

**Release Workflow:**
- Triggered on Git tags (`v*`)
- Builds binaries for all platforms
- Creates GitHub Release with artifacts

---

## Traceability & Compliance

### Traceability Matrix

| Req ID | Requirement | User Story | Milestone | Test Cases | Status |
|--------|-------------|-----------|-----------|------------|--------|
| R-1 | Window enumeration | US-1.1 | M1-M5 | T-M{1-5}-01 | ⏳ Pending |
| R-2 | Capture by selector | US-1.2 | M1-M5 | T-M{1-5}-02 | ⏳ Pending |
| R-3 | Dual-format output | US-1.3 | M1 | T-M1-01/02 | ⏳ Pending |
| R-4 | Wayland headless | US-2.1 | M2 | T-M2-01/02 | ⏳ Pending |
| R-5 | Token rotation | US-2.2 | M2 | T-M2-02/03 | ⏳ Pending |
| R-6 | X11 enumeration | US-3.1 | M3 | T-M3-01/02 | ⏳ Pending |
| R-7 | Windows WGC | US-3.2 | M4 | T-M4-02/03 | ⏳ Pending |
| R-8 | macOS TCC | US-3.3 | M5 | T-M5-01/02 | ⏳ Pending |
| R-9 | Error remediation | US-4.2 | M0-M6 | test_error_remediation | ⏳ Pending |
| R-10 | Documentation | US-4.1 | M6 | T-M6-01/02 | ⏳ Pending |

### Non-Functional Requirements

| Attribute | Target | Measurement | Owner | Priority |
|-----------|--------|-------------|-------|----------|
| Capture Latency (P95) | ≤1.5s | Tracing logs | Backend Eng | 🔴 P0 |
| Encoding Time (1080p) | PNG <300ms | Benchmarks | Backend Eng | 🟠 P1 |
| Memory Peak | <200MB | RSS monitoring | Backend Eng | 🟡 P2 |
| Binary Size | <20MB | Build output | DevOps | 🔵 P3 |
| Wayland Headless Success | ≥95% | Telemetry (opt-in) | Linux Eng | 🔴 P0 |
| Token Revocation Rate | <5% | Logs | Linux Eng | 🟠 P1 |
| Test Coverage (Unit) | ≥80% | tarpaulin | QA | 🟠 P1 |
| Platform Coverage | 4/4 | CI pass rate | PM | 🔴 P0 |

### Risks Register

| ID | Risk | Likelihood | Impact | Mitigation | Owner | Status |
|----|------|------------|--------|------------|-------|--------|
| RA-1 | `rmcp` crate issues | Medium | High | Prototype 1 day; fallback to JSON-RPC | Backend Eng | ⏳ Open |
| RA-2 | Wayland compositor fragmentation | High | Medium | Test KDE; document issues | Linux Eng | ⏳ Open |
| RA-3 | Token revocation >5% | Medium | Medium | Display+region fallback | Linux Eng | ⏳ Open |
| RA-4 | Keyring unavailable | Medium | Low | Fallback to encrypted file | Backend Eng | ⏳ Open |
| RA-5 | WGC unstable on Win10 | Medium | Medium | Version-check, graceful error | Windows Eng | ⏳ Open |
| RA-6 | macOS TCC denial | High | Low | Startup check, Settings link | macOS Eng | ⏳ Open |
| RA-7 | Slow encoding (4K) | Low | Medium | Profile, optimize, parallelize | Backend Eng | ⏳ Open |

---

## Next Steps

### Immediate (Day 1)

1. **Review & Approve Specification** ✅ (Done)
2. **Choose License:** MIT recommended (pending user final decision)
3. **Create GitHub Repository:**
   - Initialize with project layout (see M0 structure)
   - Add `.gitignore`, `rustfmt.toml`, `clippy.toml`
4. **Set Up Development Environments:**
   - Rust 1.75+ toolchain
   - KDE Plasma for Wayland testing (Linux)
   - Xvfb for X11 testing (Linux)
   - Windows 10/11 VM
   - macOS 12+ machine

### Week 1 (M0: Project Scaffold)

**Backend Engineer:**
- Initialize Cargo workspace
- Configure dependencies (`Cargo.toml`)
- Implement `main.rs` with stdio transport
- Implement `mcp.rs` with tool registry
- Define core types in `model.rs`
- Implement `health_check` tool
- Add platform detection (`util/detect.rs`)
- Write unit tests
- Configure clippy/rustfmt

**DevOps:**
- Stub CI workflow files (`.github/workflows/ci.yml`)
- Configure lint/test stages

**Exit:** `cargo build` succeeds, `health_check` works via stdio

### Week 2 (M1: Core Facade)

**Backend Engineer:**
- Define `CaptureFacade` trait (`capture/mod.rs`)
- Implement `MockBackend`
- Create `ImageBuffer` wrapper
- Implement encoding pipeline (`util/encode.rs`)
- Build MCP content builders
- Add temp file management
- Update tool stubs with mock data
- Define `CaptureError` types

**QA:**
- Write unit tests for encoding
- Write integration tests for `MockBackend`

**Exit:** Mock capture returns image+ResourceLink <2s

### Weeks 3-5 (M2-M5: Platform Backends)

**Linux Engineer:**
- **Week 3 (M2):** Wayland backend with restore tokens
- **Week 3-4 (M3):** X11 backend in parallel or sequential

**Windows Engineer:**
- **Week 4-5 (M4):** Windows WGC backend

**macOS Engineer:**
- **Week 5 (M5):** macOS ScreenCaptureKit backend

**QA:**
- Integration tests (gated) for each backend
- Performance benchmarks
- Manual E2E testing on respective platforms

**Exit:** All 4 backends functional, passing tests

### Week 6 (M6: Documentation & Release)

**Technical Writer:**
- Write comprehensive README
- Create User Guide (platform-specific setup)
- Write Troubleshooting FAQ (≥10 common issues)
- Generate API documentation (JSON Schema from code)

**DevOps:**
- Finalize CI/CD workflows
- Configure release workflow (tagged releases → artifacts)
- Document packaging roadmap (defer implementation)

**QA:**
- Manual E2E tests on all platforms
- Final validation against acceptance criteria
- Performance validation

**PM:**
- Prepare launch communication
- Post to Discord #mcp, r/rust, HackerNews

**Exit:** v1.0 released with binaries and documentation

### Post-Launch

**Support:**
- Monitor GitHub Issues (respond <24h)
- Triage and prioritize bug fixes

**PM:**
- Track adoption metrics (stars, issues)
- Gather community feedback
- Plan v2 roadmap:
  - Video capture
  - OCR
  - Interactive region selection
  - Multi-monitor enumeration
  - Linux packaging (deb, rpm, AUR, Nix)

---

## Appendices

### A. Project Structure

```
screenshot-mcp/
  Cargo.toml
  .gitignore
  rustfmt.toml
  clippy.toml
  LICENSE
  README.md
  docs/
    SPECIFICATION.md (this file)
    USER_GUIDE.md
    TROUBLESHOOTING.md
    API.md
    PACKAGING.md (roadmap)
  src/
    main.rs
    mcp.rs
    model.rs
    capture/
      mod.rs
      wayland.rs
      x11.rs
      windows.rs
      mac.rs
    util/
      key_store.rs
      detect.rs
      encode.rs
  tests/
    integration_tests.rs (gated)
  benches/
    capture_bench.rs
  examples/
    config/
      claude_desktop.json
      cursor.json
    screenshots/
      (sample outputs)
  .github/
    workflows/
      ci.yml
      release.yml
```

### B. Cargo.toml (Initial Dependencies)

```toml
[package]
name = "screenshot-mcp"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"

[dependencies]
rmcp = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
schemars = { version = "0.8", features = ["preserve_order"] }
thiserror = "1.0"
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json"] }
image = { version = "0.25", features = ["png", "jpeg", "webp"] }
tempfile = "3.10"
tokio = { version = "1.35", features = ["rt", "macros"] }
async-trait = "0.1"

[target.'cfg(target_os = "linux")'.dependencies]
ashpd = { version = "0.7", optional = true }
x11rb = { version = "0.13", optional = true }
xcap = { version = "0.0", optional = true }
keyring = { version = "2.3", optional = true }

[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.52", features = ["Win32_UI_WindowsAndMessaging", "Win32_System_Threading"] }
windows-capture = { version = "1.3", optional = true }

[target.'cfg(target_os = "macos")'.dependencies]
objc2-screen-capture-kit = { version = "0.2", optional = true }
core-graphics = { version = "0.23", optional = true }

[features]
default = ["linux-wayland", "linux-x11"]
linux-wayland = ["ashpd", "keyring"]
linux-x11 = ["x11rb", "xcap"]
windows = ["windows-capture"]
mac = ["objc2-screen-capture-kit", "core-graphics"]
integration-tests = []

[dev-dependencies]
criterion = "0.5"
```

### C. MCP Client Configuration Examples

**Claude Desktop (`config.json`):**
```json
{
  "mcpServers": {
    "screenshot": {
      "command": "/home/user/.local/bin/screenshot-mcp",
      "args": [],
      "env": {
        "RUST_LOG": "screenshot_mcp=info"
      }
    }
  }
}
```

**Cursor:**
```json
{
  "mcp": {
    "servers": {
      "screenshot": {
        "command": "/home/user/.local/bin/screenshot-mcp"
      }
    }
  }
}
```

---

## Document Control

**Version:** 1.0
**Status:** Approved
**Date:** 2024-10-13
**Author:** Specification Generator (Claude Code)
**Approver:** Project Owner

**Revision History:**

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2024-10-13 | Spec Generator | Initial specification approved |

---

**End of Specification Document**
