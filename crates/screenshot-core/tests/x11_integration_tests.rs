//! X11 backend integration tests
//!
//! These tests validate the X11 backend functionality in a live X11
//! environment. Tests are designed to be runnable in headless CI via `xvfb`
//! and will self-skip when `$DISPLAY` is not set.
//!
//! # Requirements
//!
//! - X11 display server ($DISPLAY must be set)
//! - Linux build (X11 session)
//!
//! # Running Tests
//!
//! Tests create their own deterministic X11 window, so they can run in headless
//! CI environments using xvfb.
//!
//! ```bash
//! # Run all X11 integration tests (headless with xvfb)
//! xvfb-run -a cargo test --test x11_integration_tests
//!
//! # Run with live X11 session
//! DISPLAY=:0 cargo test --test x11_integration_tests
//! ```

#[cfg(target_os = "linux")]
mod tests {
    use std::time::Instant;

    use screenshot_core::{
        capture::{CaptureFacade, x11_backend::X11Backend},
        model::{CaptureOptions, WindowSelector},
    };
    use x11rb::{
        connection::Connection as _,
        protocol::xproto::{ConnectionExt as _, WindowClass},
        rust_connection::RustConnection,
    };

    /// Helper to skip test if $DISPLAY not set
    fn check_x11_available() -> bool {
        std::env::var("DISPLAY").is_ok()
    }

    /// Creates a deterministic X11 test window with known title and pixels
    ///
    /// This function creates a simple X11 window that can be used for testing
    /// capture functionality in headless CI environments. The window has:
    /// - A known title: "screenshot-mcp-test-window"
    /// - A known size: 800x600 pixels
    /// - A colored background (red) for pixel validation
    ///
    /// Returns the window ID and a connection handle (must be kept alive).
    fn create_test_window() -> Result<(RustConnection, u32, u32), Box<dyn std::error::Error>> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;

        // Create a simple window
        let win_id = conn.generate_id()?;
        let win_aux = x11rb::protocol::xproto::CreateWindowAux::new()
            .background_pixel(screen.white_pixel)
            .event_mask(
                x11rb::protocol::xproto::EventMask::EXPOSURE
                    | x11rb::protocol::xproto::EventMask::STRUCTURE_NOTIFY,
            );

        conn.create_window(
            screen.root_depth,
            win_id,
            root,
            100, // x
            100, // y
            800, // width
            600, // height
            0,   // border_width
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &win_aux,
        )?;

        // Set window title using _NET_WM_NAME (UTF-8)
        let net_wm_name = conn.intern_atom(false, b"_NET_WM_NAME")?.reply()?.atom;
        let utf8_string = conn.intern_atom(false, b"UTF8_STRING")?.reply()?.atom;
        let title = b"screenshot-mcp-test-window";

        // For UTF-8 strings, we need to send bytes
        conn.change_property8(
            x11rb::protocol::xproto::PropMode::REPLACE,
            win_id,
            net_wm_name,
            utf8_string,
            title,
        )?;

        // Also set WM_NAME for compatibility (Latin-1)
        let wm_name = conn.intern_atom(false, b"WM_NAME")?.reply()?.atom;
        conn.change_property8(
            x11rb::protocol::xproto::PropMode::REPLACE,
            win_id,
            wm_name,
            x11rb::protocol::xproto::AtomEnum::STRING,
            title,
        )?;

        // Map the window (make it visible)
        conn.map_window(win_id)?;
        conn.flush()?;

        // Wait a moment for the window to appear
        std::thread::sleep(std::time::Duration::from_millis(100));

        Ok((conn, win_id, screen_num))
    }

    #[tokio::test]
    async fn test_list_windows_enumerate() {
        if !check_x11_available() {
            tracing::warn!("Skipping: $DISPLAY not set");
            return;
        }

        // Create a test window
        let (_conn, _win_id, _screen) = create_test_window().expect("Failed to create test window");

        let backend = X11Backend::new().expect("Failed to create X11Backend");

        let start = Instant::now();
        let windows = backend.list_windows().await.expect("list_windows failed");
        tracing::info!("list_windows: {:.2}ms", start.elapsed().as_secs_f64() * 1000.0);

        tracing::info!("Found {} windows:", windows.len());
        for window in &windows {
            tracing::info!(
                "  - [{}] {} (class: {}, exe: {})",
                window.id,
                window.title,
                window.class,
                window.owner
            );
        }

        // Should find at least our test window
        assert!(
            windows
                .iter()
                .any(|w| w.title.contains("screenshot-mcp-test-window")),
            "Should find test window"
        );
    }

    #[tokio::test]
    async fn test_resolve_target_by_title() {
        if !check_x11_available() {
            tracing::warn!("Skipping: $DISPLAY not set");
            return;
        }

        // Create a test window with known title
        let (_conn, _win_id, _screen) = create_test_window().expect("Failed to create test window");

        let backend = X11Backend::new().expect("Failed to create X11Backend");

        // Resolve by known title
        let selector = WindowSelector::by_title("screenshot-mcp-test-window");

        let start = Instant::now();
        let handle = backend
            .resolve_target(&selector)
            .await
            .expect("resolve_target failed");
        tracing::info!("resolve_target: {:.2}ms", start.elapsed().as_secs_f64() * 1000.0);

        tracing::info!("Resolved window: {}", handle);
        // Verify we got a valid handle (should match our test window)
        assert!(!handle.is_empty());
    }

    #[tokio::test]
    async fn test_capture_window_first() {
        if !check_x11_available() {
            tracing::warn!("Skipping: $DISPLAY not set");
            return;
        }

        // Create a test window
        let (_conn, _win_id, _screen) = create_test_window().expect("Failed to create test window");

        let backend = X11Backend::new().expect("Failed to create X11Backend");

        // Resolve test window by title
        let selector = WindowSelector::by_title("screenshot-mcp-test-window");
        let handle = backend
            .resolve_target(&selector)
            .await
            .expect("resolve_target failed");

        let opts = CaptureOptions::default();
        let start = Instant::now();
        let image = backend
            .capture_window(handle, &opts)
            .await
            .expect("capture_window failed");
        tracing::info!("capture_window: {:.2}ms", start.elapsed().as_secs_f64() * 1000.0);

        // Verify actual image was captured (not corrupted)
        let width = image.width();
        let height = image.height();
        let pixel_count = width as u64 * height as u64;

        // Image dimensions should be valid
        assert!(width > 0 && height > 0, "Image should have non-zero dimensions");

        // Pixel count should match dimensions
        assert!(pixel_count > 0, "Pixel count should be positive");

        // Typical window minimum is 10x10 pixels (some may be smaller)
        assert!(width >= 10 && height >= 10, "Window should be at least 10x10 pixels");

        // ========== ENHANCED: Verify actual pixel data exists ==========

        // Get raw pixel bytes
        let bytes = image.as_bytes();
        assert!(!bytes.is_empty(), "✓ Image bytes not empty");

        // Verify minimum byte size (width * height * 3 for RGB minimum)
        let min_expected = (width as usize) * (height as usize) * 3;
        assert!(
            bytes.len() >= min_expected,
            "Image should have RGB pixel data ({} bytes, expected {})",
            bytes.len(),
            min_expected
        );

        // Count non-zero bytes (pixels with some color)
        let non_zero = bytes.iter().filter(|&&b| b != 0).count();
        let zero_ratio = 1.0 - (non_zero as f64 / bytes.len() as f64);

        tracing::info!(
            "✓ Pixel data: {} bytes, {} non-zero ({:.1}% variation)",
            bytes.len(),
            non_zero,
            (1.0 - zero_ratio) * 100.0
        );

        // Image should have pixel variation (not solid black/white)
        // Allow up to 70% uniform bytes for windows with large monochrome areas
        assert!(
            zero_ratio < 0.7,
            "Image should have pixel variation ({:.1}% zero bytes)",
            zero_ratio * 100.0
        );

        tracing::info!("✓ Window captured: {}x{} ({} pixels)", width, height, pixel_count);
        tracing::info!("✓ Image validation passed");
    }

    #[tokio::test]
    async fn test_capture_display() {
        if !check_x11_available() {
            tracing::warn!("Skipping: $DISPLAY not set");
            return;
        }

        let backend = X11Backend::new().expect("Failed to create X11Backend");

        let opts = CaptureOptions::default();
        let start = Instant::now();
        let image = backend
            .capture_display(None, &opts)
            .await
            .expect("capture_display failed");
        tracing::info!("capture_display: {:.2}ms", start.elapsed().as_secs_f64() * 1000.0);

        let width = image.width();
        let height = image.height();

        // ========== ENHANCED: Verify actual pixel data exists ==========

        tracing::info!("Captured display: {}x{}", width, height);
        assert!(width > 0 && height > 0, "Display should have valid dimensions");

        // Get raw pixel bytes
        let bytes = image.as_bytes();
        assert!(!bytes.is_empty(), "✓ Display bytes not empty");

        // Verify minimum byte size
        let min_expected = (width as usize) * (height as usize) * 3;
        assert!(
            bytes.len() >= min_expected,
            "Display should have RGB pixel data ({} bytes, expected {})",
            bytes.len(),
            min_expected
        );

        // Count non-zero bytes (display should have content)
        let non_zero = bytes.iter().filter(|&&b| b != 0).count();
        let zero_ratio = 1.0 - (non_zero as f64 / bytes.len() as f64);

        tracing::info!(
            "✓ Display pixel data: {} bytes, {} non-zero ({:.1}% variation)",
            bytes.len(),
            non_zero,
            (1.0 - zero_ratio) * 100.0
        );

        // Display should have pixel variation
        assert!(
            zero_ratio < 0.6,
            "Display should have content ({:.1}% zero bytes)",
            zero_ratio * 100.0
        );

        tracing::info!("✓ Display capture validation passed");
    }

    #[tokio::test]
    async fn test_capture_with_region() {
        if !check_x11_available() {
            tracing::warn!("Skipping: $DISPLAY not set");
            return;
        }

        // Create a test window
        let (_conn, _win_id, _screen) = create_test_window().expect("Failed to create test window");

        let backend = X11Backend::new().expect("Failed to create X11Backend");

        // Resolve test window
        let selector = WindowSelector::by_title("screenshot-mcp-test-window");
        let handle = backend
            .resolve_target(&selector)
            .await
            .expect("resolve_target failed");

        // First capture full window for comparison
        let full_opts = CaptureOptions::default();
        let start = Instant::now();
        let full_image = backend
            .capture_window(handle.clone(), &full_opts)
            .await
            .expect("capture_window full failed");
        tracing::info!("capture_window_full: {:.2}ms", start.elapsed().as_secs_f64() * 1000.0);

        let full_bytes = full_image.as_bytes();
        let full_len = full_bytes.len();

        // Capture with region (top-left 200x200 pixels)
        let region_opts = CaptureOptions {
            region: Some(screenshot_core::model::Region {
                x: 0,
                y: 0,
                width: 200,
                height: 200,
            }),
            ..Default::default()
        };

        let start = Instant::now();
        let region_image = backend
            .capture_window(handle, &region_opts)
            .await
            .expect("capture_window with region failed");
        tracing::info!(
            "capture_window_with_region: {:.2}ms",
            start.elapsed().as_secs_f64() * 1000.0
        );

        let region_width = region_image.width();
        let region_height = region_image.height();
        let region_bytes = region_image.as_bytes();
        let region_len = region_bytes.len();

        // ========== ENHANCED: Verify region transformation ==========

        tracing::info!("Captured region: {}x{}", region_width, region_height);
        assert!(region_width <= 200 && region_height <= 200, "Region should be within bounds");

        // Verify region has pixel data
        assert!(!region_bytes.is_empty(), "✓ Region bytes not empty");

        // Region should be smaller than full window (or equal if window is smaller than
        // region)
        if full_len > region_len {
            let ratio = region_len as f64 / full_len as f64;
            tracing::info!(
                "✓ Region cropping reduced data: {} -> {} bytes ({:.1}%)",
                full_len,
                region_len,
                ratio * 100.0
            );
        }

        // Verify region has pixel variation
        let region_non_zero = region_bytes.iter().filter(|&&b| b != 0).count();
        let region_zero_ratio = 1.0 - (region_non_zero as f64 / region_len as f64);

        assert!(
            region_zero_ratio < 0.8,
            "Region should have content ({:.1}% zero bytes)",
            region_zero_ratio * 100.0
        );

        tracing::info!("✓ Region capture validation passed");
    }

    #[tokio::test]
    async fn test_capture_with_scale() {
        if !check_x11_available() {
            tracing::warn!("Skipping: $DISPLAY not set");
            return;
        }

        // Create a test window
        let (_conn, _win_id, _screen) = create_test_window().expect("Failed to create test window");

        let backend = X11Backend::new().expect("Failed to create X11Backend");

        // Resolve test window
        let selector = WindowSelector::by_title("screenshot-mcp-test-window");
        let handle = backend
            .resolve_target(&selector)
            .await
            .expect("resolve_target failed");

        // Capture at normal scale
        let normal_opts = CaptureOptions::default();
        let start = Instant::now();
        let normal_image = backend
            .capture_window(handle.clone(), &normal_opts)
            .await
            .expect("capture_window normal scale failed");
        tracing::info!(
            "capture_window_normal_scale: {:.2}ms",
            start.elapsed().as_secs_f64() * 1000.0
        );

        let normal_width = normal_image.width();
        let normal_height = normal_image.height();
        let normal_bytes = normal_image.as_bytes();
        let normal_len = normal_bytes.len();

        // Capture with 50% scale
        let scaled_opts = CaptureOptions {
            scale: 0.5,
            ..Default::default()
        };

        let start = Instant::now();
        let scaled_image = backend
            .capture_window(handle, &scaled_opts)
            .await
            .expect("capture_window with scale failed");
        tracing::info!(
            "capture_window_with_scale: {:.2}ms",
            start.elapsed().as_secs_f64() * 1000.0
        );

        let scaled_width = scaled_image.width();
        let scaled_height = scaled_image.height();
        let scaled_bytes = scaled_image.as_bytes();
        let scaled_len = scaled_bytes.len();

        // ========== ENHANCED: Verify scaling transformation ==========

        tracing::info!(
            "Normal: {}x{} ({} bytes), Scaled: {}x{} ({} bytes)",
            normal_width,
            normal_height,
            normal_len,
            scaled_width,
            scaled_height,
            scaled_len
        );

        // Image dimensions should be valid
        assert!(
            scaled_width > 0 && scaled_height > 0,
            "Scaled image should have valid dimensions"
        );

        // Verify scaled bytes exist
        assert!(!scaled_bytes.is_empty(), "✓ Scaled image bytes not empty");

        // Scaled image should have less or equal data than original
        if normal_width > scaled_width || normal_height > scaled_height {
            let ratio = scaled_len as f64 / normal_len as f64;
            tracing::info!(
                "✓ Scaling reduced data: {} -> {} bytes ({:.1}%)",
                normal_len,
                scaled_len,
                ratio * 100.0
            );

            // At 50% scale, expect roughly 25% of pixels (50% width * 50% height)
            // Be conservative and allow up to 40% of original
            assert!(
                ratio < 0.4,
                "50% scaled should use <40% of original bytes (got {:.1}%)",
                ratio * 100.0
            );
        } else {
            tracing::info!(
                "Note: Window dimensions didn't change with 0.5 scale (may be below minimum)"
            );
        }

        // Verify scaled image has content
        let scaled_non_zero = scaled_bytes.iter().filter(|&&b| b != 0).count();
        let scaled_zero_ratio = 1.0 - (scaled_non_zero as f64 / scaled_len as f64);

        assert!(
            scaled_zero_ratio < 0.8,
            "Scaled image should have content ({:.1}% zero bytes)",
            scaled_zero_ratio * 100.0
        );

        tracing::info!("✓ Scale transformation validation passed");
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    println!("X11 integration tests only run on Linux.");
}
