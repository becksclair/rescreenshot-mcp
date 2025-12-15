//! Shared test utilities for Windows backend integration tests
//!
//! This module provides:
//! - `WindowsTestContext`: A fixture for cleaner test setup
//! - `save_test_image`: Save images for visual verification
//! - `find_best_target_window`: Smart window selection with priority fallback
//! - `validate_image_pixels`: Pixel content validation
//! - `measure_async`: Async-native timing measurement

#![allow(dead_code)] // Helpers are available for test use, not all are always needed

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use screenshot_core::{
    capture::{CaptureFacade, ImageBuffer, windows_backend::WindowsBackend},
    model::{CaptureOptions, WindowInfo},
};

/// Get the test output directory using CARGO_MANIFEST_DIR for robustness
pub fn test_output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_output")
}

/// Save an image to test_output/ directory for visual verification
///
/// Creates the directory if needed and returns the absolute path.
pub fn save_test_image(image: &ImageBuffer, name: &str) -> PathBuf {
    use std::fs;

    let output_dir = test_output_dir();
    fs::create_dir_all(&output_dir).expect("Failed to create test_output directory");

    let filename = format!("{}.png", name);
    let path = output_dir.join(&filename);

    // Encode as PNG using image crate
    let rgba = image.to_rgba8();
    let (width, height) = image.dimensions();
    rgba.save(&path).expect("Failed to save PNG");

    let abs_path = fs::canonicalize(&path).unwrap_or(path.clone());
    println!("[SAVED] {} ({}x{}) -> {}", name, width, height, abs_path.display());

    abs_path
}

/// Validate that an image contains actual pixel data (not blank/corrupted)
pub fn validate_image_pixels(image: &ImageBuffer, name: &str) {
    let (width, height) = image.dimensions();
    let pixels = image.as_bytes();

    // Verify dimensions
    assert!(width > 0 && height > 0, "Image '{}' should have non-zero dimensions", name);

    // Count non-zero pixels
    let non_zero = pixels.iter().filter(|&&b| b != 0).count();
    let total_bytes = pixels.len();
    let zero_ratio = 1.0 - (non_zero as f64 / total_bytes as f64);

    println!(
        "[PIXELS] {} - {}x{} ({} bytes, {:.1}% non-zero)",
        name,
        width,
        height,
        total_bytes,
        (1.0 - zero_ratio) * 100.0
    );

    // Image should have some content (not all black)
    assert!(
        non_zero > 0,
        "Image '{}' should contain some pixel data (got {}/{} zero bytes)",
        name,
        total_bytes - non_zero,
        total_bytes
    );

    // Allow up to 95% zero bytes for mostly empty windows, but require some content
    assert!(
        zero_ratio < 0.95,
        "Image '{}' should have more content ({:.1}% zero bytes)",
        name,
        zero_ratio * 100.0
    );
}

/// Find the best target window for testing, with smart fallback
///
/// Priority order:
/// 1. Cursor editor
/// 2. VS Code
/// 3. Other code editors (Sublime, Notepad++, etc.)
/// 4. Browsers (Chrome, Firefox, Edge, etc.)
/// 5. Any window with substantial title
/// 6. First available window
pub fn find_best_target_window(windows: &[WindowInfo]) -> Option<&WindowInfo> {
    // Priority 1: Cursor editor
    if let Some(w) = windows.iter().find(|w| {
        w.title.to_lowercase().contains("cursor") || w.owner.to_lowercase().contains("cursor")
    }) {
        println!("[TARGET] Found Cursor: '{}' ({})", w.title, w.owner);
        return Some(w);
    }

    // Priority 2: VS Code
    if let Some(w) = windows.iter().find(|w| {
        w.title.to_lowercase().contains("visual studio code")
            || w.owner.to_lowercase().contains("code.exe")
    }) {
        println!("[TARGET] Found VS Code: '{}' ({})", w.title, w.owner);
        return Some(w);
    }

    // Priority 3: Other code editors
    let editors = [
        "sublime",
        "notepad++",
        "atom",
        "vim",
        "emacs",
        "idea",
        "webstorm",
    ];
    if let Some(w) = windows.iter().find(|w| {
        let title_lower = w.title.to_lowercase();
        let owner_lower = w.owner.to_lowercase();
        editors
            .iter()
            .any(|e| title_lower.contains(e) || owner_lower.contains(e))
    }) {
        println!("[TARGET] Found editor: '{}' ({})", w.title, w.owner);
        return Some(w);
    }

    // Priority 4: Browsers
    let browsers = ["chrome", "firefox", "edge", "brave", "opera"];
    if let Some(w) = windows.iter().find(|w| {
        let title_lower = w.title.to_lowercase();
        let owner_lower = w.owner.to_lowercase();
        browsers
            .iter()
            .any(|b| title_lower.contains(b) || owner_lower.contains(b))
    }) {
        println!("[TARGET] Found browser: '{}' ({})", w.title, w.owner);
        return Some(w);
    }

    // Priority 5: Any window with substantial title (skip system windows)
    if let Some(w) = windows.iter().find(|w| {
        w.title.len() > 5 && !w.title.starts_with("Program Manager") && !w.class.contains("Shell_")
    }) {
        println!("[TARGET] Found app: '{}' ({})", w.title, w.owner);
        return Some(w);
    }

    // Fallback: first window
    if let Some(w) = windows.first() {
        println!("[TARGET] Fallback to first: '{}' ({})", w.title, w.owner);
        Some(w)
    } else {
        None
    }
}

/// Measure the duration of an async operation
pub async fn measure_async<F, Fut, T>(name: &str, f: F) -> (T, Duration)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let start = Instant::now();
    let result = f().await;
    let elapsed = start.elapsed();
    println!("[TIMING] {}: {:.2}ms", name, elapsed.as_secs_f64() * 1000.0);
    (result, elapsed)
}

/// Measure the duration of a synchronous operation
///
/// Note: Prefer `measure_async` for async tests. This is provided for
/// backward compatibility with tests that use `futures::executor::block_on`.
pub fn measure_timing<F, T>(name: &str, f: F) -> (T, Duration)
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();
    println!("[TIMING] {}: {:.2}ms", name, elapsed.as_secs_f64() * 1000.0);
    (result, elapsed)
}

/// Test fixture for Windows backend integration tests
///
/// Provides a clean setup with pre-enumerated windows and convenience methods.
pub struct WindowsTestContext {
    pub backend: WindowsBackend,
    pub windows: Vec<WindowInfo>,
}

impl WindowsTestContext {
    /// Create a new test context with backend and window enumeration
    pub async fn new() -> Self {
        let backend = WindowsBackend::new().expect("Failed to create WindowsBackend");
        let windows = backend
            .list_windows()
            .await
            .expect("Failed to list windows");
        Self { backend, windows }
    }

    /// Find the best window for testing using smart fallback
    pub fn find_best_window(&self) -> Option<&WindowInfo> {
        find_best_target_window(&self.windows)
    }

    /// Get the first window or panic
    pub fn first_window(&self) -> &WindowInfo {
        self.windows.first().expect("No windows available")
    }

    /// Print available windows (up to limit)
    pub fn print_windows(&self, limit: usize) {
        println!("\n=== Available Windows ({}) ===", self.windows.len());
        for (i, w) in self.windows.iter().take(limit).enumerate() {
            println!(
                "  [{}] '{}' | class='{}' | owner='{}' | pid={}",
                i, w.title, w.class, w.owner, w.pid
            );
        }
        if self.windows.len() > limit {
            println!("  ... and {} more", self.windows.len() - limit);
        }
    }

    /// Capture a window by handle with options
    pub async fn capture_window(
        &self,
        handle: &str,
        opts: &CaptureOptions,
    ) -> screenshot_core::error::CaptureResult<ImageBuffer> {
        self.backend.capture_window(handle.to_string(), opts).await
    }

    /// Capture primary display with options
    pub async fn capture_display(
        &self,
        opts: &CaptureOptions,
    ) -> screenshot_core::error::CaptureResult<ImageBuffer> {
        self.backend.capture_display(None, opts).await
    }
}
