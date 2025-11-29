# Quick Start: M3 X11 Backend Image Validation Tests

## 30-Second Summary

M3 enhancement adds **5-layer pixel validation** to prove captured images contain real data:

1. Byte content exists ✓
2. Byte size matches dimensions ✓  
3. Pixels vary (not blank) ✓
4. Cropping reduces size ✓
5. Scaling reduces size ✓

**Status:** ✅ **197/197 unit tests passing**

---

## Run Unit Tests (No X11 Required)

```bash
cd /home/bex/projects/rescreenshot-mcp
cargo test --lib
```

**Expected:** `ok. 197 passed; 0 failed` ✅

Takes ~1-2 seconds. Includes the 4 new pixel validation tests.

---

## Run Integration Tests (Requires X11 + Windows)

### Prerequisites
```bash
# Check X11 is available
echo $DISPLAY  # Should show :0 or similar

# Open some windows (important!)
firefox &  # or xterm, or any GUI app
```

### Run Tests
```bash
DISPLAY=:0 cargo test --test x11_integration_tests --features linux-x11 -- --ignored --nocapture
```

### Expected Output
```
test_capture_window_first:
  ✓ Image bytes not empty
  ✓ Pixel data: 8294400 bytes, 7500000 non-zero (90.4% variation)
  ✓ Window captured: 1920x1080 (2073600 pixels)
  ✓ Image validation passed

test_capture_display:
  ✓ Display capture validation passed

test_capture_with_region:
  ✓ Region cropping reduced data: 8294400 -> 240000 bytes (2.9%)
  ✓ Region capture validation passed

test_capture_with_scale:
  ✓ Scaling reduced data: 8294400 -> 2073600 bytes (25.0%)
  ✓ Scale transformation validation passed
```

Takes ~1.5-3 seconds total.

---

## Troubleshooting

| Problem | Solution |
|---------|----------|
| "Image bytes should not be empty" | Check DISPLAY: `echo $DISPLAY`, check X11 running |
| "Image should have pixel variation" | Open a window with content (Firefox preferred) |
| "Skipping: $DISPLAY not set" | Set it: `export DISPLAY=:0` |
| Tests timeout | X11 is slow, increase timeout: `timeout 60 cargo test...` |

---

## Key Thresholds

These control what passes/fails:

**Zero Byte Ratio (must be <):**
- Display: 60%
- Window: 70%
- Cropped: 80%
- Scaled: 80%

**Transformation Ratios (must be <):**
- Region crop: 25% of original
- 50% scale: 60% of original

---

## What Gets Validated

✅ **Real pixels exist** (not fake dimensions)
✅ **Images aren't blank** (have color variation)
✅ **Transformations work** (cropping/scaling change output)
✅ **Sources differ** (window ≤ display size)
✅ **Data is consistent** (same window = similar sizes)

---

## Documentation

| File | Purpose |
|------|---------|
| `VALIDATION_QUICK_REFERENCE.md` | 1-page cheat sheet |
| `TEST_EXECUTION_GUIDE.md` | Complete how-to guide |
| `docs/IMAGE_VALIDATION_TESTING.md` | Deep technical guide |
| `M3_DELIVERY_SUMMARY.md` | Architecture & design |
| `M3_COMPLETION_CHECKLIST.md` | Verification checklist |

---

## Code Examples

### Unit Test Pattern
```rust
#[test]
fn test_captured_image_has_pixel_data() {
    // Layer 1: Bytes exist
    let bytes = image.as_bytes();
    assert!(!bytes.is_empty());
    
    // Layer 2: Size matches
    assert!(bytes.len() >= width * height * 3);
    
    // Layer 3: Has variation
    let non_zero = bytes.iter().filter(|&&b| b != 0).count();
    assert!(non_zero > bytes.len() / 2);  // >50% non-zero
}
```

### Integration Test Pattern
```rust
#[tokio::test]
#[ignore]
async fn test_capture_window_first() {
    if !check_x11_available() { return; }
    
    let backend = X11Backend::new().unwrap();
    let windows = backend.list_windows().await.unwrap();
    let image = backend.capture_window(...).await.unwrap();
    
    // All 5 validation layers
    assert!(!image.as_bytes().is_empty());
    assert!(image.as_bytes().len() >= width * height * 3);
    // ... pixel variation check
    // ... transformation checks
}
```

---

## Test Files Structure

```
src/capture/x11_backend.rs          (2560 lines)
  └─ tests::test_captured_image_has_pixel_data
  └─ tests::test_window_capture_vs_display_different_data  
  └─ tests::test_region_crop_reduces_pixel_data
  └─ tests::test_scale_transform_changes_byte_size
  └─ ... (193 existing tests)

tests/x11_integration_tests.rs       (456 lines)
  └─ test_list_windows_enumerate
  └─ test_resolve_target_by_title
  └─ test_capture_window_first           [ENHANCED]
  └─ test_capture_display                [ENHANCED]
  └─ test_capture_with_region            [ENHANCED]
  └─ test_capture_with_scale             [ENHANCED]
```

---

## Validation Logic

```
INPUT: Captured Image
  ↓
LAYER 1: Check bytes exist
  if as_bytes().is_empty() → FAIL
  ↓
LAYER 2: Check size matches dimensions
  if len < width × height × 3 → FAIL
  ↓
LAYER 3: Check pixel variation
  count non-zero bytes
  if zero_ratio > threshold → FAIL
  ↓
LAYER 4: (For transformations) Check crop reduces size
  if cropped_len >= original_len → FAIL
  ↓
LAYER 5: (For transformations) Check scale reduces size
  if scaled_len >= original_len × ratio → FAIL
  ↓
✅ PASS: Image validated as real
```

---

## Performance

**Unit Tests:** <2 seconds (197 tests)
- Per test: 1-10ms

**Integration Tests:** 1.5-3 seconds (6 tests, if X11 available)
- list_windows: ~150ms
- capture_window: ~500ms
- capture_display: ~500ms

---

## Code Quality

```bash
# All tests passing
cargo test --lib
# → ok. 197 passed; 0 failed ✅

# No warnings
cargo clippy --lib
# → [no output] ✅

# Properly formatted
cargo fmt -- --check
# → [no output] ✅
```

---

## Next: M4 Windows Backend

The same 5-layer validation pattern will be applied to:
- Windows Graphics Capture API
- Window enumeration via Win32
- Similar test structure

---

## Questions?

See documentation files:
- **Quick answer?** → `VALIDATION_QUICK_REFERENCE.md`
- **How do I run this?** → `TEST_EXECUTION_GUIDE.md`
- **Why these thresholds?** → `docs/IMAGE_VALIDATION_TESTING.md`
- **What was delivered?** → `M3_DELIVERY_SUMMARY.md`

---

**Status:** ✅ Complete  
**Tests:** 197/197 passing  
**Delivered:** 2025-11-29
