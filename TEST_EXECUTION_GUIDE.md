# M3 X11 Backend - Test Execution Guide

## Quick Start

### Run All Unit Tests (No X11 Required)
```bash
cargo test --lib
```

**Expected Output:**
```
running 197 tests
...
test result: ok. 197 passed; 0 failed
```

### Run Integration Tests (Requires Live X11)
```bash
# Set display and run
DISPLAY=:0 cargo test --test x11_integration_tests --features linux-x11 -- --ignored --nocapture

# Or with logging
RUST_LOG=debug DISPLAY=:0 cargo test --test x11_integration_tests --features linux-x11 -- --ignored --nocapture
```

---

## Test Categories

### Category 1: Unit Tests (197 total)

These validate implementation without requiring X11 display.

**Location:** `src/capture/x11_backend.rs` (1000+ lines of test code)

**Run:**
```bash
cargo test --lib
```

#### Pixel Validation Tests (NEW)

| Test | Lines | What it validates |
|------|-------|-------------------|
| `test_captured_image_has_pixel_data` | ~50 | Image bytes exist and have non-zero variation |
| `test_window_capture_vs_display_different_data` | ~70 | Window and display captures differ appropriately |
| `test_region_crop_reduces_pixel_data` | ~60 | Cropping reduces byte size <25% |
| `test_scale_transform_changes_byte_size` | ~70 | Scaling reduces byte size by expected ratio |

#### Existing Unit Tests

| Category | Count | Examples |
|----------|-------|----------|
| Environment | 1 | DISPLAY detection |
| Connection | 2 | Connection management |
| Property helpers | 4 | UTF-8, Latin-1, class, PID queries |
| Window enumeration | 3 | list_windows, empty cases |
| Window resolution | 20 | Regex, substring, fuzzy, class, exe matching |
| Capture operations | 4 | capture_window, capture_display |
| Error handling | 4 | Error mapping |
| Comprehensive | 12 | Threading, timeouts, capabilities, downcasting |
| Image validation | 2 | Dimension checks, buffer validity |
| **Total** | **52** | **+ 4 new pixel validation tests = 56** |

---

### Category 2: Integration Tests (6 total)

These require a live X11 display with windows.

**Location:** `tests/x11_integration_tests.rs`

**Run:**
```bash
DISPLAY=:0 cargo test --test x11_integration_tests --features linux-x11 -- --ignored --nocapture
```

#### Test Details

| Test | Duration | What it validates |
|------|----------|-------------------|
| `test_list_windows_enumerate` | ~50ms | Finds windows, reports metadata |
| `test_resolve_target_by_title` | ~100ms | Resolves window by title substring |
| `test_capture_window_first` | ~100-500ms | **[ENHANCED]** Captures real window pixels |
| `test_capture_display` | ~100-500ms | **[ENHANCED]** Captures real display pixels |
| `test_capture_with_region` | ~200-1000ms | **[ENHANCED]** Region crop reduces data |
| `test_capture_with_scale` | ~200-1000ms | **[ENHANCED]** Scale reduces data proportionally |

**Total Expected Runtime:** ~1.5-3 seconds on modern hardware

---

## Test Execution Workflows

### Workflow 1: Development Testing

```bash
# Quick check (unit tests only, <3 seconds)
cargo test --lib

# With verbose output
cargo test --lib -- --nocapture

# Specific test
cargo test test_captured_image_has_pixel_data -- --nocapture
```

### Workflow 2: Integration Testing

```bash
# Requires: X11 session with windows

# Open test windows first
xterm &
firefox &  # or any GUI window

# Run integration tests
DISPLAY=:0 cargo test --test x11_integration_tests --features linux-x11 -- --ignored --nocapture

# Run specific test with debug logging
RUST_LOG=screenshot_mcp=debug DISPLAY=:0 cargo test test_capture_window_first --test x11_integration_tests --features linux-x11 -- --ignored --nocapture
```

### Workflow 3: Comprehensive Validation

```bash
# 1. Unit tests
cargo test --lib

# 2. Code quality
cargo clippy --lib
cargo fmt -- --check

# 3. Documentation build
cargo doc --no-deps --lib

# 4. Integration tests (if X11 available)
if [ -n "$DISPLAY" ]; then
  DISPLAY=$DISPLAY cargo test --test x11_integration_tests --features linux-x11 -- --ignored --nocapture
else
  echo "Skipping integration tests: X11 not available"
fi
```

---

## Expected Test Output

### Unit Test Success

```
running 197 tests

...

test capture::x11_backend::tests::test_captured_image_has_pixel_data ... ok
test capture::x11_backend::tests::test_window_capture_vs_display_different_data ... ok
test capture::x11_backend::tests::test_region_crop_reduces_pixel_data ... ok
test capture::x11_backend::tests::test_scale_transform_changes_byte_size ... ok

...

test result: ok. 197 passed; 0 failed; 0 ignored; 0 measured
```

### Integration Test Success

```
running 6 tests

test tests::test_list_windows_enumerate - should panic ... ok
[INFO] Found 8 windows:
  - [1] Firefox (class: firefox, exe: firefox)
  - [2] XTerm (class: xterm, exe: xterm)
  ...

test tests::test_capture_window_first - should panic ... ok
[INFO] list_windows: 45.23ms
[INFO] ✓ Image bytes not empty
[INFO] ✓ Pixel data: 8294400 bytes, 7500000 non-zero (90.4% variation)
[INFO] ✓ Window captured: 1920x1080 (2073600 pixels)
[INFO] ✓ Image validation passed

test tests::test_capture_display - should panic ... ok
[INFO] ✓ Display pixel data: 8294400 bytes, 7200000 non-zero (86.8% variation)
[INFO] ✓ Display capture validation passed

test tests::test_capture_with_region - should panic ... ok
[INFO] ✓ Region cropping reduced data: 8294400 -> 240000 bytes (2.9%)
[INFO] ✓ Region capture validation passed

test tests::test_capture_with_scale - should panic ... ok
[INFO] ✓ Scaling reduced data: 8294400 -> 2073600 bytes (25.0%)
[INFO] ✓ Scale transformation validation passed

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured
```

---

## Troubleshooting

### Issue: Unit Tests Fail

```
thread 'test_something' panicked at 'assertion failed: ...'
```

**Solution:**
1. Check Rust version: `rustc --version` (need 1.70+)
2. Check dependencies: `cargo update`
3. Run specific test with logs: `cargo test test_name -- --nocapture`
4. Check for environment issues: `env | grep DISPLAY`

### Issue: Integration Tests Don't Run

```
Skipping: $DISPLAY not set
```

**Solution:**
```bash
# Set DISPLAY explicitly
DISPLAY=:0 cargo test --test x11_integration_tests --features linux-x11 -- --ignored --nocapture

# Or check what display you have
echo $DISPLAY
ls /tmp/.X11-unix/
```

### Issue: "Image should have pixel variation"

```
panicked at 'Image should have pixel variation (got 95.0% zero bytes, expected <50%)'
```

**Cause:** Window or display is mostly blank

**Solutions:**
1. Open colorful windows: `firefox`, `xterm`, `gimp`
2. Change desktop background to something non-black
3. Check display isn't in screensaver
4. Try capturing different window: `xdotool search --name .`

### Issue: Tests Timeout After 30 Seconds

```
error: test timed out after 30 seconds
```

**Cause:** System is slow or X11 is unresponsive

**Solution:**
```bash
# Run with longer timeout
timeout 60 cargo test --test x11_integration_tests --features linux-x11 -- --ignored --nocapture

# Or increase individually
RUST_TEST_TIME_UNIT=60000 RUST_TEST_TIME_INTEGRATION=120000 cargo test ...
```

---

## Performance Baseline

### Unit Tests
- **Total time:** 2-3 seconds
- **Per test:** 1-10ms
- **System overhead:** Minimal (no I/O or network)

### Integration Tests
- **Total time:** 1.5-3 seconds
- **Per test:** 100-500ms
- **Breakdown:**
  - list_windows: 50ms
  - resolve_target: 10-50ms
  - capture_window: 100-500ms
  - capture_display: 100-500ms

### Expected Latencies (P95)

| Operation | Latency |
|-----------|---------|
| list_windows (10 windows) | 150ms |
| resolve_target (substring) | 100ms |
| resolve_target (fuzzy) | 200ms |
| capture_window (1920x1080) | 500ms |
| capture_display (1920x1080) | 500ms |

---

## Validation Checklist

### Before Running Tests
- [ ] Rust 1.70+ installed: `rustc --version`
- [ ] X11 dependencies present: `pkg-config --list-all | grep xcb`
- [ ] For integration tests: `echo $DISPLAY` returns `:0` or similar
- [ ] For integration tests: At least one window open: `xdotool search --name .`

### After Running Unit Tests
- [ ] All 197 tests pass
- [ ] No clippy warnings: `cargo clippy --lib`
- [ ] Code formatted: `cargo fmt -- --check`
- [ ] Build succeeds: `cargo build --lib`

### After Running Integration Tests
- [ ] All 6 tests pass
- [ ] Output shows "✓ Image validation passed" for captures
- [ ] Latencies are <500ms per operation
- [ ] No hangs or timeouts
- [ ] Pixel data shown in logs (~90% variation typical)

---

## Continuous Integration

### GitHub Actions Pipeline

```yaml
name: X11 Tests
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      
      # Install X11 dev libs
      - run: sudo apt-get install -y libxcb1 libxcb-render0 libxcb-image0
      
      # Run unit tests
      - run: cargo test --lib
      
      # Build integration tests (don't run in CI, no X11 display)
      - run: cargo test --test x11_integration_tests --features linux-x11 --no-run
```

---

## Running Tests Locally

### Quick Test (Development)
```bash
cargo test --lib
```

### Full Validation (Pre-Commit)
```bash
#!/bin/bash
set -e

echo "Running unit tests..."
cargo test --lib

echo "Checking code quality..."
cargo clippy --lib -- -D warnings

echo "Checking formatting..."
cargo fmt -- --check

echo "Building docs..."
cargo doc --no-deps --lib

echo "✅ All checks passed!"
```

### Complete With Integration (Local X11 Session)
```bash
#!/bin/bash
set -e

# Unit tests
echo "1. Unit tests..."
cargo test --lib

# Code quality
echo "2. Code quality..."
cargo clippy --lib -- -D warnings
cargo fmt -- --check

# Integration tests (if X11 available)
if [ -n "$DISPLAY" ]; then
    echo "3. Integration tests..."
    DISPLAY=$DISPLAY cargo test --test x11_integration_tests --features linux-x11 -- --ignored --nocapture
else
    echo "3. Integration tests... SKIPPED (no X11 display)"
fi

echo "✅ All checks passed!"
```

---

## Test Output Interpretation

### ✅ Good Sign
```
test result: ok. 197 passed; 0 failed
✓ Pixel data: 8294400 bytes, 7500000 non-zero (90.4% variation)
```
- All tests pass
- Pixel variation >85% (colorful content)
- Byte sizes reasonable

### ⚠️ Warning Sign
```
test result: ok. 197 passed; 0 failed
✓ Pixel data: 8294400 bytes, 100000 non-zero (1.2% variation)
```
- Tests pass but barely
- Pixel variation <10% (mostly blank)
- Check if captured window is visible

### ❌ Failure Sign
```
thread 'test_captured_image_has_pixel_data' panicked at 
  'Image should have pixel variation (got 95.0% zero bytes, expected <50%)'
```
- Image is 95% black/white
- Capture failed or window is offscreen
- Verify X11 display and visible windows

---

## Advanced Testing

### Running with Custom Log Level
```bash
RUST_LOG=screenshot_mcp=trace cargo test --lib -- --nocapture

# Output includes:
# [TRACE] Reusing existing X11 connection
# [DEBUG] Interning EWMH atoms for first time
# [INFO] ✓ Image validation passed
```

### Profiling Test Performance
```bash
time cargo test --lib
# real    0m2.945s
# user    0m2.891s
# sys     0m0.145s
```

### Testing Specific Window Manager
```bash
# KDE Plasma
DISPLAY=:0 cargo test --test x11_integration_tests --features linux-x11 -- --ignored

# GNOME (with verbose output)
DISPLAY=:0 RUST_LOG=debug cargo test test_capture_window_first --test x11_integration_tests --features linux-x11 -- --ignored --nocapture

# Wayland-under-X11 (Xwayland)
DISPLAY=:1 cargo test --test x11_integration_tests --features linux-x11 -- --ignored
```

---

## Summary

| Category | Tests | Command | Time | Requires X11 |
|----------|-------|---------|------|---|
| Unit tests | 197 | `cargo test --lib` | 2-3s | ✗ |
| Integration tests | 6 | `cargo test --test x11_integration_tests --features linux-x11 -- --ignored` | 1.5-3s | ✓ |
| Code quality | N/A | `cargo clippy && cargo fmt --check` | 5-10s | ✗ |
| Docs | N/A | `cargo doc --no-deps --lib` | 5-10s | ✗ |

---

**Last Updated:** 2025-11-29
**Test Count:** 197 unit + 6 integration
**Coverage:** Pixel validation at all 5 layers
**Status:** ✅ Ready for execution
