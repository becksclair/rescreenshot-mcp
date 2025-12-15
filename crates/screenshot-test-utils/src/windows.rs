//! Windows-specific test utilities
//!
//! This module provides test fixtures and helpers for Windows backend integration tests.
//!
//! # Key Types
//!
//! - [`WindowsTestContext`]: Test fixture with pre-enumerated windows and backend
//! - [`save_test_image`]: Save captured images for visual verification
//! - [`validate_image_pixels`]: Verify image content is not blank/corrupted
//! - [`find_best_target_window`]: Smart window selection with priority fallback
//!
//! # Example
//!
//! ```ignore
//! use screenshot_test_utils::windows::{WindowsTestContext, save_test_image};
//! use screenshot_core::model::CaptureOptions;
//!
//! #[tokio::test]
//! async fn test_window_capture() {
//!     let ctx = WindowsTestContext::new().await;
//!     let window = ctx.find_best_window().expect("No windows available");
//!
//!     let image = ctx.capture_window(&window.handle, &CaptureOptions::default()).await.unwrap();
//!     save_test_image(&image, "captured_window");
//! }
//! ```

#![allow(dead_code)] // Helpers are available for test use, not all are always needed

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use screenshot_core::{
    capture::{ImageBuffer, ScreenCapture, WindowEnumerator, windows_backend::WindowsBackend},
    error::CaptureResult,
    model::{CaptureOptions, WindowInfo},
};

/// Get the test output directory using CARGO_MANIFEST_DIR for robustness
pub fn test_output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_output")
}

/// Save an image to test_output/ directory for visual verification
///
/// Creates the directory if needed and returns the absolute path.
///
/// # Example
///
/// ```ignore
/// use screenshot_test_utils::windows::save_test_image;
///
/// let path = save_test_image(&captured_image, "my_test_capture");
/// println!("Image saved to: {}", path.display());
/// ```
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
///
/// Checks that:
/// - Image has non-zero dimensions
/// - Image contains some non-zero pixels
/// - Image is not mostly empty (>5% content required)
///
/// # Panics
///
/// Panics with descriptive message if validation fails.
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
///
/// # Example
///
/// ```ignore
/// use screenshot_test_utils::windows::find_best_target_window;
///
/// let window = find_best_target_window(&windows).expect("No windows found");
/// println!("Testing with: {} ({})", window.title, window.owner);
/// ```
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
///
/// For async tests, prefer this over sync timing wrappers.
///
/// # Example
///
/// ```ignore
/// use screenshot_test_utils::windows::measure_async;
///
/// let (result, duration) = measure_async("capture", || async {
///     backend.capture_window(handle, &opts).await
/// }).await;
/// ```
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
///
/// # Example
///
/// ```ignore
/// use screenshot_test_utils::windows::WindowsTestContext;
///
/// #[tokio::test]
/// async fn test_capture() {
///     let ctx = WindowsTestContext::new().await;
///     ctx.print_windows(5); // Show first 5 windows
///
///     let window = ctx.find_best_window().unwrap();
///     let image = ctx.capture_window(&window.handle, &CaptureOptions::default()).await.unwrap();
/// }
/// ```
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
    ) -> CaptureResult<ImageBuffer> {
        self.backend.capture_window(handle.to_string(), opts).await
    }

    /// Capture primary display with options
    pub async fn capture_display(&self, opts: &CaptureOptions) -> CaptureResult<ImageBuffer> {
        self.backend.capture_display(None, opts).await
    }
}
