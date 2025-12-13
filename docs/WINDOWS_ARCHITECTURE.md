# Windows Backend Architecture Guide

> **Document Status:** Complete (M4 Phase 6)  
> **Last Updated:** 2025-12-13  
> **For Version:** screenshot-mcp with Windows Graphics Capture (WGC)

## Table of Contents

1. [High-Level Architecture](#high-level-architecture)
2. [Core Components](#core-components)
3. [Implementation Details](#implementation-details)
4. [Data Flow](#data-flow)
5. [Error Handling Strategy](#error-handling-strategy)
6. [Testing Architecture](#testing-architecture)
7. [Performance Characteristics](#performance-characteristics)
8. [Debugging & Troubleshooting](#debugging--troubleshooting)

---

## High-Level Architecture

### System Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                 screenshot-mcp Server (Rust)                │
└─────────────────────────────────────────────────────────────┘
                              ↓
                    ┌────────────────────┐
                    │  WindowsBackend    │
                    │   (CaptureFacade)  │
                    └────────────────────┘
                              ↓
         ┌────────────────────┼────────────────────┐
         ↓                    ↓                    ↓
    ┌─────────┐         ┌─────────┐         ┌──────────┐
    │  Win32  │         │   WGC   │         │ windows- │
    │ EnumWnd │         │ Capture │         │ capture  │
    │  API    │         │   API   │         │  crate   │
    └─────────┘         └─────────┘         └──────────┘
         ↓                    ↓                    ↓
    ┌────────────────────────────────────────────────────┐
    │  Windows Kernel / Graphics Driver / GPU            │
    └────────────────────────────────────────────────────┘
```

### Design Principles

**Stateless Backend**
- No persistent window handles or connections
- All enumeration is on-demand via Win32 APIs
- No polling or background threads
- Thread-safe: `Send + Sync` for use with `Arc`

**Layered Abstraction**
1. **Windows Kernel:** Win32 APIs (EnumWindows, GetWindowText, etc.)
2. **Graphics Capture API:** WGC via windows-capture crate
3. **Rust Bindings:** windows-sys, windows-capture
4. **Application Layer:** CaptureFacade trait implementation

**Fail-Fast Validation**
- Windows version checked at capture time (build 17134+)
- Window handle validated before capture attempt
- Metadata available immediately (no async enumeration)
- Timeout protection on all operations

---

## Core Components

### 1. WindowsBackend Struct

```rust
#[derive(Debug)]
pub struct WindowsBackend {
    _private: (),  // Stateless design
}
```

**Key Features:**
- Zero overhead (unit type)
- Fully `Send + Sync`
- Can be shared via `Arc<WindowsBackend>` across threads
- New instances created on-demand (cheap allocation)

**Lifecycle:**
```
new() -> WindowsBackend (zero-cost)
↓
list_windows() -> uses Win32 EnumWindows (transient)
↓
resolve_target() -> matches window by criteria (transient)
↓
capture_window() -> frames from WGC (transient)
↓
capture_display() -> frames from WGC (transient)
```

### 2. Win32 Enumeration System

**Window Handle Collection**
```rust
fn enumerate_window_handles() -> Vec<HWND>
```

**Process:**
1. Call Win32 `EnumWindows` with callback
2. For each HWND:
   - Check visibility via `IsWindowVisible()`
   - Check for title via `GetWindowTextLength()`
   - Collect valid handles
3. Return vector of HWNDs

**Filtering Rules:**
- ✓ Visible windows only
- ✓ Windows with titles
- ✗ Hidden system windows
- ✗ Titleless windows

**Metadata Extraction**
```rust
fn fetch_window_info(hwnd: HWND) -> Option<WindowInfo>
```

For each valid HWND:
- **Title:** `GetWindowTextW()` (Unicode UTF-16 → String)
- **Class:** `GetClassNameW()` (Window class identifier)
- **Process ID:** `GetWindowThreadProcessId()` (PID)
- **Executable:** `GetModuleBaseNameW()` (Process name)

**Error Handling:**
- Access denied → gracefully skipped
- Invalid HWND → gracefully skipped
- Null pointer → empty string returned

### 3. Window Matching Strategies

**Hierarchical Matching (First Match Wins):**

```
1. Regex Pattern Matching
   ↓ (pattern invalid)
2. Substring Matching (Case-Insensitive)
   ↓ (no match)
3. Exact Class Matching (Case-Insensitive)
   ↓ (no match)
4. Exact Exe Matching (Case-Insensitive)
   ↓ (no match)
5. Fuzzy Matching (Typo-Tolerant)
   ↓ (no match)
Error: WindowNotFound
```

**Strategy Details:**

| Strategy | Input | Pattern | Case | Notes |
|----------|-------|---------|------|-------|
| Regex | title_substring_or_regex | Regex | No | 1MB limit (ReDoS safety) |
| Substring | title_substring_or_regex | Literal | No | Fast, simple matching |
| Class | class | Exact | No | Window class identifier |
| Exe | exe | Exact | No | Process executable name |
| Fuzzy | title_substring_or_regex | Skim | No | Threshold: 60/100 |

### 4. Windows Graphics Capture (WGC)

**Initialization Flow**

```
1. Parse HWND from string ID
2. Check Windows version (registry)
3. Validate window exists (IsWindow)
4. Create WgcWindow from HWND
5. Configure capture settings (cursor, etc.)
6. Initialize frame capture handler
7. Start WGC session
8. Wait for frame (timeout)
9. Convert BGRA → RGBA
10. Apply transformations (crop, scale)
11. Return ImageBuffer
```

**Key Components:**

**Frame Handler**
```rust
struct FrameHandler {
    frame: Option<DynamicImage>,
    error: Option<CaptureError>,
}

impl GraphicsCaptureApiHandler for FrameHandler {
    fn on_frame_arrived(&mut self, frame: Frame) -> Result<()> {
        // Convert BGRA to RGBA
        // Store in self.frame
        Ok(())
    }
}
```

**Capture Session**
```rust
let mut session = CaptureSession::new(window, settings)?;
session.start()?;
// Wait for on_frame_arrived callback (max 2s)
session.stop()?;
```

**Frame Conversion**
```
WGC BGRA Format:
┌──┬──┬──┬──┐
│B │G │R │A │  (Blue-Green-Red-Alpha)
└──┴──┴──┴──┘
     ↓ (swap R and B)
Standard RGBA Format:
┌──┬──┬──┬──┐
│R │G │B │A │  (Red-Green-Blue-Alpha)
└──┴──┴──┴──┘
```

---

## Implementation Details

### Version Checking

**Why Registry-Based Checking?**
- WGC requires Windows 10 v1803+ (build 17134)
- Proactive detection before capture attempt
- Avoids runtime WGC initialization failure

**Registry Path:**
```
HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion
Value: CurrentBuildNumber (REG_SZ, e.g., "22631")
```

**Logic:**
```rust
fn check_wgc_available() -> CaptureResult<()> {
    let build = get_windows_build();  // Query registry
    if build < 17134 {
        Err(CaptureError::UnsupportedWindowsVersion {
            current_build: build,
            minimum_build: 17134,
        })
    } else {
        Ok(())
    }
}
```

**Build Number Examples:**
- Windows 10 v1803 (April 2018): build 17134
- Windows 10 v21H2 (latest): build 19045
- Windows 11 v21H2: build 22000
- Windows 11 v23H2: build 22631

### Timeout Protection

**Rationale:**
WGC is frame-based and can hang on:
- Unresponsive overlay applications (Discord, OBS)
- GPU driver hang
- Compositing pipeline deadlock
- Large frame allocation (4K, 8K displays)

**Timeout Constants:**
```rust
const LIST_WINDOWS_TIMEOUT_MS: u64 = 1500;       // Enumeration
const CAPTURE_WINDOW_TIMEOUT_MS: u64 = 2000;     // Capture
```

**Timeout Implementation:**
```rust
async fn with_timeout<F, T>(future: F, timeout_ms: u64) -> CaptureResult<T>
where
    F: std::future::Future<Output = CaptureResult<T>>,
{
    tokio::time::timeout(Duration::from_millis(timeout_ms), future)
        .await
        .map_err(|_| CaptureError::CaptureTimeout {
            duration_ms: timeout_ms,
        })?
}
```

### Transformations

**Region Cropping**
```
Original Window: 1920x1080
Region Request: (x:100, y:100, w:400, h:300)
                ↓
Cropped Image: 400x300 (pixels 100-499 horizontal, 100-399 vertical)
```

**Implementation:**
```rust
if let Some(region) = &options.region {
    image = crop_pixels(&image, region.x, region.y, region.width, region.height);
}
```

**Scaling**
```
Original Window: 1920x1080
Scale Factor: 0.5
                ↓
Scaled Image: 960x540 (bilinear resampling)
```

**Implementation:**
```rust
if options.scale != 1.0 {
    let new_width = (width as f64 * options.scale) as u32;
    let new_height = (height as f64 * options.scale) as u32;
    image = resize(&image, new_width, new_height, FilterType::Lanczos3);
}
```

**Transformation Order:**
```
Raw Capture → [Crop] → [Scale] → ImageBuffer
```

Note: Cropping before scaling produces more efficient results.

---

## Data Flow

### Window Enumeration Flow

```
┌─────────────────────────────────────────┐
│ Request: list_windows()                 │
└─────────────────────────────────────────┘
           ↓
┌─────────────────────────────────────────┐
│ Win32 EnumWindows (callback each HWND)  │
└─────────────────────────────────────────┘
           ↓
      For each HWND:
      ├─ IsWindowVisible? → filter
      ├─ GetWindowTextLength > 0? → filter
      └─ Fetch metadata (title, class, exe, pid)
           ↓
┌─────────────────────────────────────────┐
│ Return Vec<WindowInfo>                  │
│ {                                       │
│   id: "12345",                          │
│   title: "Notepad",                     │
│   class: "Notepad",                     │
│   owner: "notepad.exe",                 │
│   pid: 5678,                            │
│ }                                       │
└─────────────────────────────────────────┘
```

**Complexity Analysis:**
- Time: O(n) where n = number of windows
- Space: O(n) for result vector
- Latency: ~30ms per window (title + class + exe extraction)

### Window Resolution Flow

```
┌─────────────────────────────────────────┐
│ Request: resolve_target(selector)       │
└─────────────────────────────────────────┘
           ↓
Enumerate windows (cache result)
           ↓
Match by priority:
  1. Regex pattern? → return first match
  2. Substring match? → return first match
  3. Exact class? → return first match
  4. Exact exe? → return first match
  5. Fuzzy match? → return highest score
           ↓
┌─────────────────────────────────────────┐
│ Return window ID (HWND as string)       │
│ Example: "1234567890"                   │
└─────────────────────────────────────────┘
    OR
┌─────────────────────────────────────────┐
│ Error: WindowNotFound                   │
└─────────────────────────────────────────┘
```

### Capture Flow

```
┌─────────────────────────────────────────────────┐
│ Request: capture_window(hwnd, options)          │
└─────────────────────────────────────────────────┘
           ↓
1. Parse HWND from string
           ↓
2. Check Windows version
   (if build < 17134: error)
           ↓
3. Validate window (IsWindow check)
   (if closed: error)
           ↓
4. Create WGC window from HWND
           ↓
5. Configure WGC settings:
   ├─ cursor_capture: options.include_cursor
   ├─ color_format: BGRA
   └─ timeout: 2000ms
           ↓
6. Spawn blocking task (sync WGC → async Tokio)
   └─ Initialize WGC session
   └─ Start capture
   └─ Wait for frame (max 2s)
   └─ Convert BGRA → RGBA
           ↓
7. Apply transformations (if requested):
   ├─ Crop region
   └─ Scale image
           ↓
┌─────────────────────────────────────────────────┐
│ Return ImageBuffer (RgbaImage)                  │
└─────────────────────────────────────────────────┘
    OR (Error path)
┌─────────────────────────────────────────────────┐
│ Error: WindowClosed / BackendNotAvailable / ... │
└─────────────────────────────────────────────────┘
```

---

## Error Handling Strategy

### Error Classification

**Four Categories:**

1. **Configuration Errors** (client-side)
   - Invalid window selector
   - Invalid region bounds
   - Invalid scale factor

2. **Environment Errors** (system-level)
   - Windows version too old
   - WGC unavailable
   - Graphics driver issues

3. **State Errors** (runtime)
   - Window closed/destroyed
   - Invalid window handle
   - Permission denied

4. **Timeout Errors** (performance)
   - Enumeration timeout
   - Capture timeout
   - GPU hang

### Error Mapping

**Win32 → CaptureError**

| Win32 Condition | CaptureError | Remediation |
|-----------------|--------------|-------------|
| build < 17134 | UnsupportedWindowsVersion | Update Windows |
| IsWindow returns FALSE | WindowClosed | Retry or use display |
| WGC init fails | BackendNotAvailable | Update drivers |
| Timeout exceeded | CaptureTimeout | Check GPU/drivers |
| No matching window | WindowNotFound | Try different selector |
| Invalid selector | InvalidParameter | Provide title/class/exe |

### Example: Window Closed During Capture

```
1. Enumerate windows:
   ├─ Notepad HWND: 0x12345

2. Resolve by title:
   └─ Match found: 0x12345

3. Capture window:
   ├─ Validate: IsWindow(0x12345) → TRUE
   ├─ User closes Notepad
   ├─ Initialize WGC:
   │  └─ IsWindow(0x12345) → FALSE
   │  └─ Create fails
   └─ Return: WindowClosed error

4. Client can:
   ├─ Retry with different window
   ├─ Use display capture instead
   └─ Re-enumerate and try again
```

---

## Testing Architecture

### Unit Tests (73 total)

**Categories:**

| Category | Count | Examples |
|----------|-------|----------|
| Window Enumeration | 8 | `test_enumerate_windows_sync`, filters hidden windows |
| Window Matching | 20 | Regex, substring, fuzzy, class, exe matching |
| Version Checking | 3 | Registry read, build comparison, error variants |
| Edge Cases | 15 | Unicode, emoji, special characters, invalid handles |
| Transformations | 8 | Crop boundaries, scale factors, transformations |
| Error Handling | 12 | All error mapping paths |
| Thread Safety | 2 | Send + Sync trait validation |
| Constants | 5 | Timeout values, build requirements |

**Location:** `src/capture/windows_backend.rs`

**Example Test:**
```rust
#[test]
fn test_substring_match_unicode_characters() {
    let windows = vec![WindowInfo {
        title: "文档 - Notepad".to_string(),  // Chinese characters
        ..Default::default()
    }];

    let result = WindowsBackend::try_substring_match("文档", &windows);
    assert_eq!(result, Some("1".to_string()));
}
```

### Integration Tests (21 total)

**Location:** `tests/windows_integration_tests.rs`

**Requirements:**
- Windows 10 v1803+ or Windows 11
- Live desktop with open windows
- DirectX 11+ runtime
- Graphics driver with WGC support

**Running Integration Tests:**
```powershell
# Run all
cargo test --test windows_integration_tests --features windows-backend -- --ignored --nocapture

# Run specific test
cargo test test_capture_window_pixel_validation --features windows-backend -- --ignored --nocapture

# With debug logging
$env:RUST_LOG = "screenshot_mcp=debug"
cargo test --test windows_integration_tests --features windows-backend -- --ignored --nocapture
```

**Test Categories:**

| Test | Purpose | Validates |
|------|---------|-----------|
| `test_list_windows_enumeration_valid` | Window enumeration | Timing, metadata, parsing |
| `test_resolve_target_multiple_strategies` | Window matching | All 5 matching strategies |
| `test_capture_window_pixel_validation` | Window capture | Pixel data, timing |
| `test_capture_display_pixel_validation` | Display capture | Pixel data, dimensions |
| `test_capture_window_region_dimensions` | Region cropping | Bounds, dimensions |
| `test_capture_window_scaling_dimensions` | Scaling | Dimension reduction |
| `test_capture_window_with_cursor` | Cursor inclusion | Flag handling |
| `test_capture_window_multiple_times` | Caching behavior | Performance progression |
| `test_capture_invalid_window_handle` | Error handling | Invalid handle rejection |
| `test_capabilities_report` | Capability reporting | Feature flags |

**Pixel Validation Helper:**
```rust
fn validate_image_pixels(image: &RgbaImage, name: &str) {
    let (width, height) = image.dimensions();
    let pixels = image.as_raw();
    
    // Check dimensions
    assert!(width > 0 && height > 0);
    
    // Check pixel data exists
    let non_zero = pixels.iter().filter(|&&b| b != 0).count();
    assert!(non_zero > 0, "Image should have non-zero pixels");
    
    // Check pixel variation (not all same color)
    let zero_ratio = 1.0 - (non_zero as f64 / pixels.len() as f64);
    assert!(zero_ratio < 0.95, "Image should have content variation");
}
```

---

## Performance Characteristics

### Typical Latencies

**Window Enumeration**
```
Operation:           list_windows()
Typical Count:       20-50 windows
P50 Latency:        100ms
P95 Latency:        300ms
Timeout:            1500ms
```

**Window Resolution**
```
Operation:          resolve_target()
Strategy 1 (Regex):  50ms
Strategy 2 (Substr): 40ms
Strategy 3 (Fuzzy):  200-800ms (depends on window count)
Timeout:            2000ms
```

**Window Capture**
```
Size:               1920x1080 (FHD)
P50 Latency:        50-100ms
P95 Latency:        200-400ms
Timeout:            2000ms

Size:               3840x2160 (4K)
P50 Latency:        150-200ms
P95 Latency:        500-800ms
Timeout:            2000ms (may timeout with weak GPU)
```

**Display Capture**
```
Size:               1920x1080 (FHD)
P50 Latency:        50-100ms
P95 Latency:        200-400ms
Timeout:            2000ms
```

### Bottleneck Analysis

**CPU-Bound:**
- Regex matching: O(n) windows × pattern compilation
- Metadata extraction: `GetWindowTextW`, `GetClassNameW`
- Image transformation: Scaling (bilinear resampling)

**GPU-Bound:**
- Frame capture: GPU memory allocation, compositing
- Large captures: 4K+ frames, multiple displays

**I/O-Bound:**
- Registry access: Windows version check (1-2 ms)
- Window enumeration: EnumWindows callback overhead

### Optimization Tips

**Reduce Enumeration Cost:**
- Cache `list_windows()` result when possible
- Reuse WindowsBackend instance across multiple captures
- Avoid repeated regex compilation (compile once, reuse)

**Improve Capture Performance:**
- Scale down images (2x scale = 1/4 pixels)
- Crop to region of interest
- Use `capture_display` instead of `capture_window` for full-screen content

**GPU Optimization:**
- Update graphics drivers regularly
- Close overlay applications (Discord, OBS)
- Disable compositing effects if possible (Windows Aero)

---

## Debugging & Troubleshooting

### Debug Logging

**Enable Debug Logs:**
```powershell
$env:RUST_LOG = "screenshot_mcp=debug"
cargo run --features windows-backend
```

**Expected Log Output:**
```
[DEBUG] Enumerated 32 window handles
[DEBUG] Created window for capture from HWND: 0x12345
[DEBUG] Windows build number: 22631
[DEBUG] WGC available: OK
[WARN]  Windows operation timed out after 2000ms
[INFO]  Resolved window: 0x12345
```

### Common Issues

**Issue: "UnsupportedWindowsVersion"**
```
Cause: Windows 10 < v1803 or Windows 7/8
Fix:   Update to Windows 10 v1803 or later
Check: Settings > System > About
```

**Issue: "BackendNotAvailable"**
```
Cause: Graphics driver missing, corrupt, or outdated
Fix:   Update GPU driver from manufacturer website
Check: Settings > Device Manager > Display adapters
```

**Issue: "CaptureTimeout" on 4K displays**
```
Cause: GPU too weak for large frames, driver hang
Fix:   Scale down image (use 0.5x scale), close overlays
Check: Task Manager > Performance tab
```

**Issue: "WindowNotFound"**
```
Cause: Window title/class/exe doesn't match
Fix:   List windows first, use exact substring
Example: backend.list_windows().await → copy exact title
```

**Issue: Capture returns all-black image**
```
Cause: Exclusive fullscreen application, unresponsive app
Fix:   Exit fullscreen mode, restart application
Check: Alt+Tab to switch windows
```

### Diagnostic Tools

**DirectX Diagnostics:**
```powershell
dxdiag
# Verify:
# - DirectX 12 installed
# - Graphics driver dates recent
# - No Device Manager errors
```

**Windows Update:**
```powershell
Settings > System > About
# Current OS build should be 17134 or higher
# Install pending updates if available
```

**Performance Monitor:**
```powershell
perfmon
# Monitor:
# - GPU usage (should spike during capture)
# - CPU usage (should be low)
# - Memory usage (should be <200MB peak)
```

**Event Viewer (Graphics Driver Errors):**
```powershell
eventvwr
# Check:
# - Windows Logs > System
# - Search for GPU/Display driver errors
```

---

## Related Documentation

- [Windows Backend Implementation](windows_backend.md) - Feature details and API reference
- [Testing Guide](TESTING.md) - Comprehensive testing procedures
- [Image Validation](IMAGE_VALIDATION_TESTING.md) - Pixel validation framework
- [Error Reference](API.md) - Complete error type documentation

---

## FAQ

**Q: Why use WGC instead of DXGI or GDI?**
A: WGC is GPU-accelerated (faster), handles compositing (transparency, effects), and works with Remote Desktop. DXGI is low-level and complex; GDI is slow.

**Q: Can I capture fullscreen exclusive games?**
A: No. Fullscreen exclusive apps bypass the compositor. Use display capture instead.

**Q: How do I capture windows from other user sessions?**
A: Not possible. UAC and session isolation prevent cross-session access.

**Q: Why check Windows build via registry?**
A: Fast, proactive detection before WGC initialization fails at runtime.

**Q: Does WGC work over RDP (Remote Desktop)?**
A: Yes, but performance depends on bandwidth and GPU passthrough.

**Q: How do I optimize for many rapid captures?**
A: Reuse WindowsBackend instance and avoid re-enumerating windows frequently.

---

## References

- [Windows Graphics Capture Documentation](https://docs.microsoft.com/windows/win32/direct3d12/taking-a-screenshot)
- [windows-capture Crate](https://docs.rs/windows-capture/)
- [Win32 API Reference](https://docs.microsoft.com/windows/win32/api/)
- [Registry Hives Documentation](https://docs.microsoft.com/windows/win32/sysinfo/registry-hives)
- [DirectX 12 Graphics](https://docs.microsoft.com/windows/win32/direct3d12/)

---

**Document Version:** 2.0  
**Audience:** Developers, maintainers, DevOps engineers  
**Maintained By:** screenshot-mcp Team
