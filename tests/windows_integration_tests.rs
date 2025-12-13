//! Windows backend integration tests
//!
//! These tests require a Windows desktop environment with actual windows and
//! open applications. All tests are marked `#[ignore]` for manual execution
//! with a live Windows desktop.
//!
//! # Running Tests
//!
//! ```powershell
//! # Run all Windows integration tests with full output
//! cargo test --test windows_integration_tests --features windows-backend -- --ignored --nocapture
//!
//! # Run specific test
//! cargo test --test windows_integration_tests test_capture_window_real --features windows-backend -- --ignored --nocapture
//!
//! # Run with debug logging
//! set RUST_LOG=screenshot_mcp=debug
//! cargo test --test windows_integration_tests --features windows-backend -- --ignored --nocapture
//! ```
//!
//! # Requirements
//!
//! - Windows 10 version 1803 (April 2018 Update) or later
//! - Windows 11 recommended
//! - At least 2-3 visible windows open (Notepad, Explorer, etc.)
//! - Graphics Capture API support (enabled by default on modern Windows)
//! - DirectX 11+ runtime
//!
//! # Test Environment Checklist
//!
//! Before running integration tests:
//! - [ ] Open Windows Explorer or File Explorer
//! - [ ] Open Notepad or another text application
//! - [ ] Ensure primary display is visible
//! - [ ] Disable exclusive fullscreen games/applications
//! - [ ] Verify Windows is updated (Settings > Update & Security)
//! - [ ] Check DirectX installation (dxdiag)

#![cfg(feature = "windows-backend")]

mod common;

use screenshot_mcp::{
    capture::{windows_backend::WindowsBackend, CaptureFacade},
    model::{CaptureOptions, Region, WindowSelector},
};

// Import common helpers
use common::windows_helpers::{
    measure_timing, save_test_image, validate_image_pixels, WindowsTestContext,
};

/// Test that we can enumerate windows on the system
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_list_windows_real() {
    let backend = WindowsBackend::new().expect("Failed to create backend");
    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");

    println!("Found {} windows:", windows.len());
    for window in &windows {
        println!(
            "  HWND={} title='{}' class='{}' owner='{}' pid={}",
            window.id, window.title, window.class, window.owner, window.pid
        );
    }

    assert!(!windows.is_empty(), "Expected at least one window");
}

/// Test that we can resolve a window by title
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_resolve_target_real() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    // First list windows to find one we can target
    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");
    let first_window = windows.first().expect("No windows available");

    println!("Targeting window: {}", first_window.title);

    // Try to resolve by title
    let selector = WindowSelector::by_title(&first_window.title);
    let handle = backend
        .resolve_target(&selector)
        .await
        .expect("Failed to resolve target");

    assert_eq!(handle, first_window.id);
    println!("Resolved to handle: {}", handle);
}

/// Test that we can capture a window
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_capture_window_real() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    // Get first window
    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");
    let first_window = windows.first().expect("No windows available");

    println!("Capturing window: {} ({})", first_window.title, first_window.id);

    let opts = CaptureOptions::default();
    let image = backend
        .capture_window(first_window.id.clone(), &opts)
        .await
        .expect("Failed to capture window");

    let (width, height) = image.dimensions();
    println!("Captured image: {}x{} pixels", width, height);

    assert!(width > 0, "Image width should be > 0");
    assert!(height > 0, "Image height should be > 0");

    // Validate we have actual pixel data (not all zeros)
    let raw = image.to_rgba8();
    let has_nonzero = raw.iter().any(|&b| b != 0);
    assert!(has_nonzero, "Image should contain non-zero pixel data");
}

/// Test that we can capture the primary display
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_capture_display_real() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    println!("Capturing primary display...");

    let opts = CaptureOptions::default();
    let image = backend
        .capture_display(None, &opts)
        .await
        .expect("Failed to capture display");

    let (width, height) = image.dimensions();
    println!("Captured display: {}x{} pixels", width, height);

    assert!(width > 0, "Image width should be > 0");
    assert!(height > 0, "Image height should be > 0");

    // Validate we have actual pixel data
    let raw = image.to_rgba8();
    let has_nonzero = raw.iter().any(|&b| b != 0);
    assert!(has_nonzero, "Image should contain non-zero pixel data");
}

/// Test window capture with region cropping
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_capture_with_region() {
    use screenshot_mcp::model::Region;

    let backend = WindowsBackend::new().expect("Failed to create backend");

    // Get first window
    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");
    let first_window = windows.first().expect("No windows available");

    let opts = CaptureOptions {
        region: Some(Region {
            x:      10,
            y:      10,
            width:  100,
            height: 100,
        }),
        ..Default::default()
    };

    let image = backend
        .capture_window(first_window.id.clone(), &opts)
        .await
        .expect("Failed to capture window with region");

    let (width, height) = image.dimensions();
    println!("Captured region: {}x{} pixels", width, height);

    assert_eq!(width, 100, "Region width should be 100");
    assert_eq!(height, 100, "Region height should be 100");
}

/// Test window capture with scaling
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_capture_with_scale() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    // Get first window
    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");
    let first_window = windows.first().expect("No windows available");

    // First capture at full size
    let full_opts = CaptureOptions::default();
    let full_image = backend
        .capture_window(first_window.id.clone(), &full_opts)
        .await
        .expect("Failed to capture full window");

    let (full_width, full_height) = full_image.dimensions();
    println!("Full size: {}x{}", full_width, full_height);

    // Then capture at half scale
    let half_opts = CaptureOptions {
        scale: 0.5,
        ..Default::default()
    };

    let half_image = backend
        .capture_window(first_window.id.clone(), &half_opts)
        .await
        .expect("Failed to capture scaled window");

    let (half_width, half_height) = half_image.dimensions();
    println!("Half size: {}x{}", half_width, half_height);

    // Allow for rounding differences
    assert!(
        (half_width as f64 - full_width as f64 * 0.5).abs() <= 1.0,
        "Scaled width should be approximately half"
    );
    assert!(
        (half_height as f64 - full_height as f64 * 0.5).abs() <= 1.0,
        "Scaled height should be approximately half"
    );
}

/// Test window matching by class name
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_resolve_by_class() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    // Get first window's class
    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");
    let first_window = windows.first().expect("No windows available");

    println!(
        "Looking for class: {} (from window '{}')",
        first_window.class, first_window.title
    );

    let selector = WindowSelector::by_class(&first_window.class);
    let handle = backend
        .resolve_target(&selector)
        .await
        .expect("Failed to resolve by class");

    println!("Resolved to handle: {}", handle);
    // May not match the same window, but should find something with that class
}

/// Test window matching by executable name
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_resolve_by_exe() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    // Get first window's owner
    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");
    let first_window = windows.first().expect("No windows available");

    if first_window.owner.is_empty() {
        println!("Skipping test: first window has no owner");
        return;
    }

    println!("Looking for exe: {} (from window '{}')", first_window.owner, first_window.title);

    let selector = WindowSelector::by_exe(&first_window.owner);
    let handle = backend
        .resolve_target(&selector)
        .await
        .expect("Failed to resolve by exe");

    println!("Resolved to handle: {}", handle);
}

// ============ Enhanced Integration Tests (Window & Display Validation)
// ============
// Helper functions moved to common::windows_helpers module

/// Enhanced: Test list_windows returns valid enumeration
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_list_windows_enumeration_valid() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    let (windows, elapsed) = measure_timing("list_windows", || {
        futures::executor::block_on(backend.list_windows()).expect("Failed to list windows")
    });

    println!("Enumerated {} windows:", windows.len());
    for (i, win) in windows.iter().take(5).enumerate() {
        println!(
            "  [{}] id={} title='{}' class='{}' owner='{}' pid={}",
            i, win.id, win.title, win.class, win.owner, win.pid
        );
    }
    if windows.len() > 5 {
        println!("  ... and {} more", windows.len() - 5);
    }

    // Assertions
    assert!(!windows.is_empty(), "Should enumerate at least one window");
    assert!(elapsed.as_secs_f64() < 2.0, "Enumeration should complete in <2s");

    // All windows should have valid metadata
    for win in &windows {
        assert!(!win.id.is_empty(), "Window should have ID");
        assert!(!win.title.is_empty(), "Window should have title (filtered in enum)");
        let _id_as_isize: isize = win
            .id
            .parse()
            .expect("Window ID should be parseable as isize");
    }
}

/// Enhanced: Test resolve_target with various strategies
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_resolve_target_multiple_strategies() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");
    assert!(!windows.is_empty(), "Should have windows to test against");

    // Test with first window
    let target_win = &windows[0];
    println!(
        "Testing with window: {} (class: {}, owner: {})",
        target_win.title, target_win.class, target_win.owner
    );

    // Strategy 1: By full title
    if !target_win.title.is_empty() {
        let selector = WindowSelector::by_title(&target_win.title);
        let result = backend.resolve_target(&selector).await;
        assert!(result.is_ok(), "Should resolve by full title");
        println!("✓ Resolved by full title");
    }

    // Strategy 2: By title substring
    if target_win.title.len() > 3 {
        let substr = &target_win.title[0..target_win.title.len().min(10)];
        let selector = WindowSelector::by_title(substr);
        let result = backend.resolve_target(&selector).await;
        println!(
            "  Title substring '{}': {}",
            substr,
            if result.is_ok() { "OK" } else { "Failed" }
        );
    }

    // Strategy 3: By class (if available)
    if !target_win.class.is_empty() {
        let selector = WindowSelector::by_class(&target_win.class);
        let result = backend.resolve_target(&selector).await;
        println!(
            "  Class match '{}': {}",
            target_win.class,
            if result.is_ok() { "OK" } else { "Failed" }
        );
    }

    // Strategy 4: By exe (if available)
    if !target_win.owner.is_empty() {
        let selector = WindowSelector::by_exe(&target_win.owner);
        let result = backend.resolve_target(&selector).await;
        println!(
            "  Exe match '{}': {}",
            target_win.owner,
            if result.is_ok() { "OK" } else { "Failed" }
        );
    }
}

/// Enhanced: Test capture_window with pixel validation
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_capture_window_pixel_validation() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");
    let target_win = windows.first().expect("Should have at least one window");

    println!("Capturing window: {} ({})", target_win.title, target_win.id);

    let (image, elapsed) = measure_timing("capture_window", || {
        futures::executor::block_on(
            backend.capture_window(target_win.id.clone(), &CaptureOptions::default()),
        )
        .expect("Failed to capture window")
    });

    let (width, height) = image.dimensions();
    println!("Captured: {}x{} pixels", width, height);

    // Timing validation
    assert!(elapsed.as_secs_f64() < 3.0, "Capture should complete in <3s");

    // Pixel validation
    validate_image_pixels(&image, &format!("{}x{}", width, height));
}

/// Enhanced: Test capture_display with pixel validation
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_capture_display_pixel_validation() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    println!("Capturing primary display...");

    let (image, elapsed) = measure_timing("capture_display", || {
        futures::executor::block_on(backend.capture_display(None, &CaptureOptions::default()))
            .expect("Failed to capture display")
    });

    let (width, height) = image.dimensions();
    println!("Captured display: {}x{} pixels", width, height);

    // Timing validation
    assert!(elapsed.as_secs_f64() < 3.0, "Display capture should complete in <3s");

    // Display should be at least 640x480 on any reasonable system
    assert!(width >= 640 && height >= 480, "Display should be at least 640x480");

    // Pixel validation
    validate_image_pixels(&image, "display");
}

/// Enhanced: Test capture with region cropping produces correct dimensions
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_capture_window_region_dimensions() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");
    let target_win = windows.first().expect("Should have at least one window");

    // First capture at full size
    let full_image = backend
        .capture_window(target_win.id.clone(), &CaptureOptions::default())
        .await
        .expect("Failed to capture full window");

    let (full_width, full_height) = full_image.dimensions();
    println!("Full window: {}x{}", full_width, full_height);

    // Capture a region (smaller than window)
    let region_width = (full_width / 2).max(50);
    let region_height = (full_height / 2).max(50);

    let opts = CaptureOptions {
        region: Some(Region {
            x:      0,
            y:      0,
            width:  region_width,
            height: region_height,
        }),
        ..Default::default()
    };

    let (region_image, _) = measure_timing("capture_window_with_region", || {
        futures::executor::block_on(backend.capture_window(target_win.id.clone(), &opts))
            .expect("Failed to capture with region")
    });

    let (actual_width, actual_height) = region_image.dimensions();
    println!(
        "Region capture: {}x{} (requested: {}x{})",
        actual_width, actual_height, region_width, region_height
    );

    // Verify dimensions match requested region (or are bounded by window size)
    assert!(
        actual_width <= region_width && actual_height <= region_height,
        "Region should not exceed requested dimensions"
    );

    // Should be smaller or equal to full image
    assert!(
        actual_width <= full_width && actual_height <= full_height,
        "Region should be within original window"
    );

    validate_image_pixels(&region_image, "region");
}

/// Enhanced: Test capture with scaling produces smaller image
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_capture_window_scaling_dimensions() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");
    let target_win = windows.first().expect("Should have at least one window");

    // Capture at normal scale
    let normal_image = backend
        .capture_window(target_win.id.clone(), &CaptureOptions::default())
        .await
        .expect("Failed to capture at normal scale");

    let (normal_width, normal_height) = normal_image.dimensions();
    println!("Normal scale: {}x{}", normal_width, normal_height);

    // Capture at 0.5x scale
    let scaled_opts = CaptureOptions {
        scale: 0.5,
        ..Default::default()
    };

    let (scaled_image, _) = measure_timing("capture_window_scaled", || {
        futures::executor::block_on(backend.capture_window(target_win.id.clone(), &scaled_opts))
            .expect("Failed to capture at 0.5x scale")
    });

    let (scaled_width, scaled_height) = scaled_image.dimensions();
    println!("Scaled (0.5x): {}x{}", scaled_width, scaled_height);

    // Scaled image should be approximately half the size (allow rounding error)
    if normal_width > 100 && normal_height > 100 {
        let expected_width = (normal_width as f64 * 0.5) as u32;
        let expected_height = (normal_height as f64 * 0.5) as u32;

        assert!(
            (scaled_width as i32 - expected_width as i32).abs() <= 2,
            "Scaled width should be ~{}, got {}",
            expected_width,
            scaled_width
        );

        assert!(
            (scaled_height as i32 - expected_height as i32).abs() <= 2,
            "Scaled height should be ~{}, got {}",
            expected_height,
            scaled_height
        );

        validate_image_pixels(&scaled_image, "scaled");
    }
}

/// Enhanced: Test that cursor capture works when enabled
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_capture_window_with_cursor() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");
    let target_win = windows.first().expect("Should have at least one window");

    let mut opts = CaptureOptions::default();
    opts.include_cursor = true;

    let (image, elapsed) = measure_timing("capture_window_with_cursor", || {
        futures::executor::block_on(backend.capture_window(target_win.id.clone(), &opts))
            .expect("Failed to capture with cursor")
    });

    println!("Captured with cursor in {:.2}ms", elapsed.as_secs_f64() * 1000.0);

    validate_image_pixels(&image, "with cursor");

    // Cursor capture should not significantly slow down capture
    assert!(elapsed.as_secs_f64() < 3.0, "Capture with cursor should still complete in <3s");
}

/// Enhanced: Test multiple captures of same window (caching behavior)
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_capture_window_multiple_times() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    let windows = backend
        .list_windows()
        .await
        .expect("Failed to list windows");
    let target_win = windows.first().expect("Should have at least one window");

    println!("Capturing same window 3 times to observe caching effects");

    let opts = CaptureOptions::default();

    // First capture (may be slower due to WGC initialization)
    let (image1, time1) = measure_timing("capture 1", || {
        futures::executor::block_on(backend.capture_window(target_win.id.clone(), &opts))
            .expect("Failed to capture 1")
    });

    // Second capture (should be faster)
    let (image2, time2) = measure_timing("capture 2", || {
        futures::executor::block_on(backend.capture_window(target_win.id.clone(), &opts))
            .expect("Failed to capture 2")
    });

    // Third capture (should be consistent)
    let (_image3, time3) = measure_timing("capture 3", || {
        futures::executor::block_on(backend.capture_window(target_win.id.clone(), &opts))
            .expect("Failed to capture 3")
    });

    println!(
        "Timing progression: {:.2}ms -> {:.2}ms -> {:.2}ms",
        time1.as_secs_f64() * 1000.0,
        time2.as_secs_f64() * 1000.0,
        time3.as_secs_f64() * 1000.0
    );

    // Verify all captures have similar content
    assert_eq!(image1.dimensions(), image2.dimensions(), "Captures should have same dimensions");

    // All should be valid
    validate_image_pixels(&image1, "capture 1");
    validate_image_pixels(&image2, "capture 2");
}

/// Enhanced: Test error handling for invalid/closed windows
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_capture_invalid_window_handle() {
    let backend = WindowsBackend::new().expect("Failed to create backend");

    // Try to capture with clearly invalid handle
    let result = backend
        .capture_window("0xdeadbeef".to_string(), &CaptureOptions::default())
        .await;

    println!("Capture with invalid handle returned: {:?}", result.is_err());
    assert!(result.is_err(), "Should fail when capturing invalid window handle");
}

/// Enhanced: Capabilities check
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_capabilities_report() {
    let backend = WindowsBackend::new().expect("Failed to create backend");
    let caps = backend.capabilities();

    println!("Windows Backend Capabilities:");
    println!("  Window Capture: {}", caps.supports_window_capture);
    println!("  Display Capture: {}", caps.supports_display_capture);
    println!("  Region Crop: {}", caps.supports_region);
    println!("  Cursor: {}", caps.supports_cursor);
    println!("  Wayland Restore: {}", caps.supports_wayland_restore);

    // Windows should support all except Wayland
    assert!(caps.supports_window_capture, "Should support window capture");
    assert!(caps.supports_display_capture, "Should support display capture");
    assert!(caps.supports_region, "Should support region cropping");
    assert!(caps.supports_cursor, "Should support cursor capture");
    assert!(!caps.supports_wayland_restore, "Should not require Wayland restore");
}

// ============ Visual Verification Tests ============
// These tests save screenshots to test_output/ for AI visual analysis
// Helper functions are in common::windows_helpers module

/// Visual test: Capture a target window (Cursor preferred) and save for verification
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_visual_capture_target_window() {
    let ctx = WindowsTestContext::new().await;
    ctx.print_windows(15);

    // Find best target
    let target = ctx.find_best_window().expect("No windows available");

    println!("\n=== Capturing Window ===");
    println!("Title: {}", target.title);
    println!("Class: {}", target.class);
    println!("Owner: {}", target.owner);
    println!("Handle: {}", target.id);

    // Capture
    let image = ctx
        .capture_window(&target.id, &CaptureOptions::default())
        .await
        .expect("Failed to capture window");

    let (width, height) = image.dimensions();
    println!("Captured: {}x{} pixels", width, height);

    // Validate and save
    validate_image_pixels(&image, "window");
    let path = save_test_image(&image, "window_capture");
    println!("\n[VERIFY] Screenshot saved to: {}", path.display());
}

/// Visual test: Capture the primary display and save for verification
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_visual_capture_display() {
    let ctx = WindowsTestContext::new().await;

    println!("\n=== Capturing Primary Display ===");

    let image = ctx
        .capture_display(&CaptureOptions::default())
        .await
        .expect("Failed to capture display");

    let (width, height) = image.dimensions();
    println!("Display size: {}x{} pixels", width, height);

    // Validate
    assert!(
        width >= 640 && height >= 480,
        "Display should be at least 640x480"
    );
    validate_image_pixels(&image, "display");

    // Save for visual verification
    let path = save_test_image(&image, "display_capture");
    println!("\n[VERIFY] Screenshot saved to: {}", path.display());
}

/// Visual test: Capture with various options (region, scale, cursor)
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_visual_capture_with_options() {
    let ctx = WindowsTestContext::new().await;
    let target = ctx.find_best_window().expect("No windows available");

    println!("\n=== Capturing With Options ===");
    println!("Target: '{}' ({})", target.title, target.id);
    let handle = target.id.clone();

    // 1. Capture at half scale
    println!("\n--- Half Scale (0.5x) ---");
    let scaled_image = ctx
        .capture_window(
            &handle,
            &CaptureOptions {
                scale: 0.5,
                ..Default::default()
            },
        )
        .await
        .expect("Failed to capture scaled");

    let (sw, sh) = scaled_image.dimensions();
    println!("Scaled size: {}x{}", sw, sh);
    validate_image_pixels(&scaled_image, "scaled");
    let scaled_path = save_test_image(&scaled_image, "window_scaled_50pct");

    // 2. Capture with cursor
    println!("\n--- With Cursor ---");
    let cursor_image = ctx
        .capture_window(
            &handle,
            &CaptureOptions {
                include_cursor: true,
                ..Default::default()
            },
        )
        .await
        .expect("Failed to capture with cursor");

    let (cw, ch) = cursor_image.dimensions();
    println!("With cursor: {}x{}", cw, ch);
    validate_image_pixels(&cursor_image, "with_cursor");
    let cursor_path = save_test_image(&cursor_image, "window_with_cursor");

    // 3. Capture center region
    println!("\n--- Center Region (400x300) ---");
    let full_image = ctx
        .capture_window(&handle, &CaptureOptions::default())
        .await
        .expect("Failed to capture full");

    let (fw, fh) = full_image.dimensions();
    let region_w = 400.min(fw);
    let region_h = 300.min(fh);
    let region_x = (fw.saturating_sub(region_w)) / 2;
    let region_y = (fh.saturating_sub(region_h)) / 2;

    let region_image = ctx
        .capture_window(
            &handle,
            &CaptureOptions {
                region: Some(Region {
                    x:      region_x,
                    y:      region_y,
                    width:  region_w,
                    height: region_h,
                }),
                ..Default::default()
            },
        )
        .await
        .expect("Failed to capture region");

    let (rw, rh) = region_image.dimensions();
    println!(
        "Region: {}x{} (from {}x{} at {},{})",
        rw, rh, region_w, region_h, region_x, region_y
    );
    validate_image_pixels(&region_image, "region");
    let region_path = save_test_image(&region_image, "window_center_region");

    // Summary
    println!("\n=== Saved Screenshots ===");
    println!("1. Scaled (50%): {}", scaled_path.display());
    println!("2. With cursor: {}", cursor_path.display());
    println!("3. Center region: {}", region_path.display());
}
