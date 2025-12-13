//! Windows backend integration tests
//!
//! These tests require a Windows desktop environment with actual windows.
//! All tests are marked `#[ignore]` for manual execution.
//!
//! # Running Tests
//!
//! ```powershell
//! cargo test --test windows_integration_tests --features windows-backend -- --ignored --nocapture
//! ```
//!
//! # Requirements
//!
//! - Windows 10 version 1803 or later
//! - At least one visible window open
//! - Graphics Capture API support (enabled by default on supported Windows)

#![cfg(feature = "windows-backend")]

use screenshot_mcp::{
    capture::{windows_backend::WindowsBackend, CaptureFacade},
    model::{CaptureOptions, WindowSelector},
};

/// Test that we can enumerate windows on the system
#[tokio::test]
#[ignore = "requires Windows desktop environment"]
async fn test_list_windows_real() {
    let backend = WindowsBackend::new().expect("Failed to create backend");
    let windows = backend.list_windows().await.expect("Failed to list windows");

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
    let windows = backend.list_windows().await.expect("Failed to list windows");
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
    let windows = backend.list_windows().await.expect("Failed to list windows");
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
    let windows = backend.list_windows().await.expect("Failed to list windows");
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
    let windows = backend.list_windows().await.expect("Failed to list windows");
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
    let windows = backend.list_windows().await.expect("Failed to list windows");
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
    let windows = backend.list_windows().await.expect("Failed to list windows");
    let first_window = windows.first().expect("No windows available");

    if first_window.owner.is_empty() {
        println!("Skipping test: first window has no owner");
        return;
    }

    println!(
        "Looking for exe: {} (from window '{}')",
        first_window.owner, first_window.title
    );

    let selector = WindowSelector::by_exe(&first_window.owner);
    let handle = backend
        .resolve_target(&selector)
        .await
        .expect("Failed to resolve by exe");

    println!("Resolved to handle: {}", handle);
}
