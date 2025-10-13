//! Mock capture backend for testing
//!
//! This module provides a `MockBackend` implementation of the [`CaptureFacade`]
//! trait for testing and development purposes. The mock backend generates
//! synthetic test images and simulates window enumeration without requiring
//! access to a real windowing system.
//!
//! # Features
//!
//! - **Synthetic Image Generation:** Creates test pattern images at specified
//!   dimensions
//! - **Mock Window List:** Provides 3 predefined mock windows (Firefox, VSCode,
//!   Terminal)
//! - **Fuzzy Matching:** Supports window selector matching by title (case-
//!   insensitive), class, and executable name
//! - **Configurable Delay:** Simulate async operation delays for testing
//! - **Error Injection:** Inject errors to test error handling paths
//! - **Full Capabilities:** Supports all capture features (cursor, region,
//!   scaling, etc.)
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```
//! use screenshot_mcp::{
//!     capture::{mock::MockBackend, CaptureFacade},
//!     model::{CaptureOptions, WindowSelector},
//! };
//!
//! #[tokio::main]
//! async fn main() {
//!     // Create mock backend
//!     let backend = MockBackend::new();
//!
//!     // List mock windows
//!     let windows = backend.list_windows().await.unwrap();
//!     assert_eq!(windows.len(), 3);
//!
//!     // Capture a window
//!     let selector = WindowSelector::by_title("Firefox");
//!     let handle = backend.resolve_target(&selector).await.unwrap();
//!     let opts = CaptureOptions::default();
//!     let image = backend.capture_window(handle, &opts).await.unwrap();
//!     assert_eq!(image.dimensions(), (1920, 1080));
//! }
//! ```
//!
//! ## With Configurable Delay
//!
//! ```
//! use std::time::Duration;
//!
//! use screenshot_mcp::{
//!     capture::{mock::MockBackend, CaptureFacade},
//!     model::CaptureOptions,
//! };
//!
//! #[tokio::main]
//! async fn main() {
//!     // Simulate 100ms delay for all operations
//!     let backend = MockBackend::new().with_delay(Duration::from_millis(100));
//!
//!     let windows = backend.list_windows().await.unwrap();
//!     // This took ~100ms
//! }
//! ```
//!
//! ## With Error Injection
//!
//! ```
//! use screenshot_mcp::{
//!     capture::{mock::MockBackend, CaptureFacade},
//!     error::CaptureError,
//!     model::{BackendType, WindowSelector},
//! };
//!
//! #[tokio::main]
//! async fn main() {
//!     // Inject permission denied error
//!     let error = CaptureError::PermissionDenied {
//!         platform: "test".to_string(),
//!         backend:  BackendType::None,
//!     };
//!     let backend = MockBackend::new().with_error(error);
//!
//!     // All operations will fail with the injected error
//!     let result = backend.list_windows().await;
//!     assert!(result.is_err());
//! }
//! ```

use std::time::Duration;

use async_trait::async_trait;
use tokio::time::sleep;

use super::{CaptureFacade, ImageBuffer};
use crate::{
    error::{CaptureError, CaptureResult},
    model::{BackendType, Capabilities, CaptureOptions, WindowHandle, WindowInfo, WindowSelector},
};

/// Mock capture backend for testing and development
///
/// Implements [`CaptureFacade`] without requiring access to a real windowing
/// system. Generates synthetic test images and provides predefined mock window
/// data.
///
/// # Thread Safety
///
/// `MockBackend` is thread-safe and can be shared across tasks using `Arc`.
///
/// # Performance
///
/// Without delay configuration, all operations complete in <10ms. With the
/// `with_delay()` builder, you can simulate realistic async operation times.
#[derive(Debug)]
pub struct MockBackend {
    /// Optional delay to simulate async operation timing
    delay:           Option<Duration>,
    /// Optional error to inject for testing error handling
    error_injection: Option<CaptureError>,
    /// Predefined mock windows
    windows:         Vec<WindowInfo>,
}

impl MockBackend {
    /// Creates a new MockBackend with default mock windows
    ///
    /// The backend includes 3 predefined windows:
    /// - Firefox (Navigator class, pid 1000)
    /// - VSCode (Code class, pid 2000)
    /// - Terminal (Alacritty class, pid 3000)
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::capture::mock::MockBackend;
    ///
    /// let backend = MockBackend::new();
    /// ```
    pub fn new() -> Self {
        Self {
            delay:           None,
            error_injection: None,
            windows:         Self::create_mock_windows(),
        }
    }

    /// Sets a configurable delay for all async operations
    ///
    /// Useful for testing timeout handling and simulating realistic async
    /// operation timing.
    ///
    /// # Arguments
    ///
    /// * `delay` - Duration to sleep before returning from async methods
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    ///
    /// use screenshot_mcp::capture::mock::MockBackend;
    ///
    /// let backend = MockBackend::new().with_delay(Duration::from_millis(100));
    /// ```
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    /// Injects an error that will be returned by all operations
    ///
    /// Useful for testing error handling paths without needing real error
    /// conditions.
    ///
    /// # Arguments
    ///
    /// * `error` - The error to inject and return from all operations
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::{capture::mock::MockBackend, error::CaptureError, model::BackendType};
    ///
    /// let error = CaptureError::PermissionDenied {
    ///     platform: "test".to_string(),
    ///     backend:  BackendType::None,
    /// };
    /// let backend = MockBackend::new().with_error(error);
    /// ```
    pub fn with_error(mut self, error: CaptureError) -> Self {
        self.error_injection = Some(error);
        self
    }

    /// Creates the predefined mock windows
    fn create_mock_windows() -> Vec<WindowInfo> {
        vec![
            WindowInfo::new(
                "mock-0x1".to_string(),
                "Mozilla Firefox".to_string(),
                "Navigator".to_string(),
                "firefox".to_string(),
                1000,
                BackendType::None,
            ),
            WindowInfo::new(
                "mock-0x2".to_string(),
                "Visual Studio Code".to_string(),
                "Code".to_string(),
                "code".to_string(),
                2000,
                BackendType::None,
            ),
            WindowInfo::new(
                "mock-0x3".to_string(),
                "Terminal - Alacritty".to_string(),
                "Alacritty".to_string(),
                "alacritty".to_string(),
                3000,
                BackendType::None,
            ),
        ]
    }

    /// Applies configured delay if set
    async fn apply_delay(&self) {
        if let Some(duration) = self.delay {
            sleep(duration).await;
        }
    }

    /// Checks if an error should be injected
    fn check_error_injection(&self) -> CaptureResult<()> {
        if let Some(ref error) = self.error_injection {
            // Clone the error for return
            return Err(match error {
                CaptureError::WindowNotFound { selector } => CaptureError::WindowNotFound {
                    selector: selector.clone(),
                },
                CaptureError::PortalUnavailable { portal } => CaptureError::PortalUnavailable {
                    portal: portal.clone(),
                },
                CaptureError::PermissionDenied { platform, backend } => {
                    CaptureError::PermissionDenied {
                        platform: platform.clone(),
                        backend:  *backend,
                    }
                }
                CaptureError::EncodingFailed { format, reason } => CaptureError::EncodingFailed {
                    format: format.clone(),
                    reason: reason.clone(),
                },
                CaptureError::CaptureTimeout { duration_ms } => CaptureError::CaptureTimeout {
                    duration_ms: *duration_ms,
                },
                CaptureError::InvalidParameter { parameter, reason } => {
                    CaptureError::InvalidParameter {
                        parameter: parameter.clone(),
                        reason:    reason.clone(),
                    }
                }
                CaptureError::BackendNotAvailable { backend } => {
                    CaptureError::BackendNotAvailable { backend: *backend }
                }
                CaptureError::IoError(e) => {
                    CaptureError::IoError(std::io::Error::new(e.kind(), e.to_string()))
                }
                CaptureError::ImageError(msg) => CaptureError::ImageError(msg.clone()),
                CaptureError::KeyringUnavailable { reason } => CaptureError::KeyringUnavailable {
                    reason: reason.clone(),
                },
                CaptureError::KeyringOperationFailed { operation, reason } => {
                    CaptureError::KeyringOperationFailed {
                        operation: operation.clone(),
                        reason:    reason.clone(),
                    }
                }
                CaptureError::TokenNotFound { source_id } => CaptureError::TokenNotFound {
                    source_id: source_id.clone(),
                },
                CaptureError::EncryptionFailed { reason } => CaptureError::EncryptionFailed {
                    reason: reason.clone(),
                },
            });
        }
        Ok(())
    }

    /// Performs fuzzy matching to find a window by selector
    ///
    /// Matching rules:
    /// - Title: Case-insensitive substring match
    /// - Class: Exact case-sensitive match
    /// - Exe: Exact case-sensitive match
    /// - Multiple criteria: All must match (AND logic)
    fn fuzzy_match_window(&self, selector: &WindowSelector) -> Option<&WindowInfo> {
        self.windows.iter().find(|window| {
            let title_matches = selector
                .title_substring_or_regex
                .as_ref()
                .map(|pattern| {
                    window
                        .title
                        .to_lowercase()
                        .contains(&pattern.to_lowercase())
                })
                .unwrap_or(true);

            let class_matches = selector
                .class
                .as_ref()
                .map(|class| window.class == *class)
                .unwrap_or(true);

            let exe_matches = selector
                .exe
                .as_ref()
                .map(|exe| window.owner == *exe)
                .unwrap_or(true);

            title_matches && class_matches && exe_matches
        })
    }

    /// Validates that a window handle exists in the mock window list
    fn validate_handle(&self, handle: &WindowHandle) -> CaptureResult<()> {
        if self.windows.iter().any(|w| w.id == *handle) {
            Ok(())
        } else {
            Err(CaptureError::WindowNotFound {
                selector: WindowSelector {
                    title_substring_or_regex: Some(format!("handle:{}", handle)),
                    class: None,
                    exe: None,
                },
            })
        }
    }

    /// Applies transformations to an image buffer based on capture options
    fn apply_transformations(
        &self,
        mut image: ImageBuffer,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        // Apply scaling if needed
        if (opts.scale - 1.0).abs() > f32::EPSILON {
            image = image.scale(opts.scale)?;
        }

        // Apply cropping if needed
        if let Some(region) = opts.region {
            image = image.crop(region)?;
        }

        Ok(image)
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CaptureFacade for MockBackend {
    async fn list_windows(&self) -> CaptureResult<Vec<WindowInfo>> {
        self.apply_delay().await;
        self.check_error_injection()?;
        Ok(self.windows.clone())
    }

    async fn resolve_target(&self, selector: &WindowSelector) -> CaptureResult<WindowHandle> {
        self.apply_delay().await;
        self.check_error_injection()?;

        self.fuzzy_match_window(selector)
            .map(|window| window.id.clone())
            .ok_or_else(|| CaptureError::WindowNotFound {
                selector: selector.clone(),
            })
    }

    async fn capture_window(
        &self,
        handle: WindowHandle,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        self.apply_delay().await;
        self.check_error_injection()?;
        self.validate_handle(&handle)?;

        // Generate test image at 1920x1080 (standard window size)
        let image = ImageBuffer::from_test_pattern(1920, 1080);

        // Apply transformations
        self.apply_transformations(image, opts)
    }

    async fn capture_display(
        &self,
        _display_id: Option<u32>,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        self.apply_delay().await;
        self.check_error_injection()?;

        // Generate test image at 2560x1440 (common display resolution)
        let image = ImageBuffer::from_test_pattern(2560, 1440);

        // Apply transformations
        self.apply_transformations(image, opts)
    }

    fn capabilities(&self) -> Capabilities {
        // Mock backend supports all features
        Capabilities::full()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::model::{ImageFormat, Region};

    #[test]
    fn test_mock_backend_new() {
        let backend = MockBackend::new();
        assert_eq!(backend.windows.len(), 3);
        assert!(backend.delay.is_none());
        assert!(backend.error_injection.is_none());
    }

    #[test]
    fn test_mock_backend_default() {
        let backend = MockBackend::default();
        assert_eq!(backend.windows.len(), 3);
    }

    #[test]
    fn test_mock_backend_with_delay() {
        let delay = Duration::from_millis(50);
        let backend = MockBackend::new().with_delay(delay);
        assert_eq!(backend.delay, Some(delay));
    }

    #[test]
    fn test_mock_backend_with_error() {
        let error = CaptureError::PermissionDenied {
            platform: "test".to_string(),
            backend:  BackendType::None,
        };
        let backend = MockBackend::new().with_error(error);
        assert!(backend.error_injection.is_some());
    }

    #[test]
    fn test_mock_windows_data() {
        let backend = MockBackend::new();
        let windows = &backend.windows;

        assert_eq!(windows[0].title, "Mozilla Firefox");
        assert_eq!(windows[0].class, "Navigator");
        assert_eq!(windows[0].owner, "firefox");
        assert_eq!(windows[0].pid, 1000);

        assert_eq!(windows[1].title, "Visual Studio Code");
        assert_eq!(windows[1].class, "Code");
        assert_eq!(windows[1].owner, "code");
        assert_eq!(windows[1].pid, 2000);

        assert_eq!(windows[2].title, "Terminal - Alacritty");
        assert_eq!(windows[2].class, "Alacritty");
        assert_eq!(windows[2].owner, "alacritty");
        assert_eq!(windows[2].pid, 3000);
    }

    #[tokio::test]
    async fn test_list_windows() {
        let backend = MockBackend::new();
        let windows = backend.list_windows().await.unwrap();
        assert_eq!(windows.len(), 3);
    }

    #[tokio::test]
    async fn test_list_windows_with_error_injection() {
        let error = CaptureError::BackendNotAvailable {
            backend: BackendType::None,
        };
        let backend = MockBackend::new().with_error(error);

        let result = backend.list_windows().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_target_by_title() {
        let backend = MockBackend::new();
        let selector = WindowSelector::by_title("Firefox");
        let handle = backend.resolve_target(&selector).await.unwrap();
        assert_eq!(handle, "mock-0x1");
    }

    #[tokio::test]
    async fn test_resolve_target_by_title_case_insensitive() {
        let backend = MockBackend::new();
        let selector = WindowSelector::by_title("firefox");
        let handle = backend.resolve_target(&selector).await.unwrap();
        assert_eq!(handle, "mock-0x1");
    }

    #[tokio::test]
    async fn test_resolve_target_by_title_partial_match() {
        let backend = MockBackend::new();
        let selector = WindowSelector::by_title("Visual Studio");
        let handle = backend.resolve_target(&selector).await.unwrap();
        assert_eq!(handle, "mock-0x2");
    }

    #[tokio::test]
    async fn test_resolve_target_by_class() {
        let backend = MockBackend::new();
        let selector = WindowSelector::by_class("Code");
        let handle = backend.resolve_target(&selector).await.unwrap();
        assert_eq!(handle, "mock-0x2");
    }

    #[tokio::test]
    async fn test_resolve_target_by_exe() {
        let backend = MockBackend::new();
        let selector = WindowSelector::by_exe("alacritty");
        let handle = backend.resolve_target(&selector).await.unwrap();
        assert_eq!(handle, "mock-0x3");
    }

    #[tokio::test]
    async fn test_resolve_target_not_found() {
        let backend = MockBackend::new();
        let selector = WindowSelector::by_title("Nonexistent");
        let result = backend.resolve_target(&selector).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CaptureError::WindowNotFound { .. }));
    }

    #[tokio::test]
    async fn test_resolve_target_multiple_criteria() {
        let backend = MockBackend::new();
        let selector = WindowSelector {
            title_substring_or_regex: Some("Code".to_string()),
            class: Some("Code".to_string()),
            exe: None,
        };
        let handle = backend.resolve_target(&selector).await.unwrap();
        assert_eq!(handle, "mock-0x2");
    }

    #[tokio::test]
    async fn test_resolve_target_with_error_injection() {
        let error = CaptureError::CaptureTimeout { duration_ms: 5000 };
        let backend = MockBackend::new().with_error(error);

        let selector = WindowSelector::by_title("Firefox");
        let result = backend.resolve_target(&selector).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_capture_window_default_opts() {
        let backend = MockBackend::new();
        let handle = "mock-0x1".to_string();
        let opts = CaptureOptions::default();

        let image = backend.capture_window(handle, &opts).await.unwrap();
        assert_eq!(image.dimensions(), (1920, 1080));
    }

    #[tokio::test]
    async fn test_capture_window_with_scale() {
        let backend = MockBackend::new();
        let handle = "mock-0x1".to_string();
        let opts = CaptureOptions::builder().scale(0.5).build();

        let image = backend.capture_window(handle, &opts).await.unwrap();
        assert_eq!(image.dimensions(), (960, 540));
    }

    #[tokio::test]
    async fn test_capture_window_with_crop() {
        let backend = MockBackend::new();
        let handle = "mock-0x1".to_string();
        let region = Region::new(100, 100, 800, 600);
        let opts = CaptureOptions::builder().region(region).build();

        let image = backend.capture_window(handle, &opts).await.unwrap();
        assert_eq!(image.dimensions(), (800, 600));
    }

    #[tokio::test]
    async fn test_capture_window_with_scale_and_crop() {
        let backend = MockBackend::new();
        let handle = "mock-0x1".to_string();
        let region = Region::new(50, 50, 400, 300);
        let opts = CaptureOptions::builder().scale(0.5).region(region).build();

        let image = backend.capture_window(handle, &opts).await.unwrap();
        // First scales to 960x540, then crops
        assert_eq!(image.dimensions(), (400, 300));
    }

    #[tokio::test]
    async fn test_capture_window_invalid_handle() {
        let backend = MockBackend::new();
        let handle = "invalid-handle".to_string();
        let opts = CaptureOptions::default();

        let result = backend.capture_window(handle, &opts).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CaptureError::WindowNotFound { .. }));
    }

    #[tokio::test]
    async fn test_capture_window_with_error_injection() {
        let error = CaptureError::PermissionDenied {
            platform: "test".to_string(),
            backend:  BackendType::None,
        };
        let backend = MockBackend::new().with_error(error);

        let handle = "mock-0x1".to_string();
        let opts = CaptureOptions::default();
        let result = backend.capture_window(handle, &opts).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_capture_display() {
        let backend = MockBackend::new();
        let opts = CaptureOptions::default();

        let image = backend.capture_display(None, &opts).await.unwrap();
        assert_eq!(image.dimensions(), (2560, 1440));
    }

    #[tokio::test]
    async fn test_capture_display_with_id() {
        let backend = MockBackend::new();
        let opts = CaptureOptions::default();

        let image = backend.capture_display(Some(1), &opts).await.unwrap();
        assert_eq!(image.dimensions(), (2560, 1440));
    }

    #[tokio::test]
    async fn test_capture_display_with_scale() {
        let backend = MockBackend::new();
        let opts = CaptureOptions::builder().scale(0.5).build();

        let image = backend.capture_display(None, &opts).await.unwrap();
        assert_eq!(image.dimensions(), (1280, 720));
    }

    #[tokio::test]
    async fn test_capture_display_with_crop() {
        let backend = MockBackend::new();
        let region = Region::new(200, 200, 1000, 800);
        let opts = CaptureOptions::builder().region(region).build();

        let image = backend.capture_display(None, &opts).await.unwrap();
        assert_eq!(image.dimensions(), (1000, 800));
    }

    #[tokio::test]
    async fn test_capture_display_with_error_injection() {
        let error = CaptureError::BackendNotAvailable {
            backend: BackendType::Wayland,
        };
        let backend = MockBackend::new().with_error(error);

        let opts = CaptureOptions::default();
        let result = backend.capture_display(None, &opts).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_capabilities() {
        let backend = MockBackend::new();
        let caps = backend.capabilities();

        assert!(caps.supports_cursor);
        assert!(caps.supports_region);
        assert!(caps.supports_wayland_restore);
        assert!(caps.supports_window_capture);
        assert!(caps.supports_display_capture);
    }

    #[tokio::test]
    async fn test_delay_timing() {
        let delay = Duration::from_millis(50);
        let backend = MockBackend::new().with_delay(delay);

        let start = Instant::now();
        let _ = backend.list_windows().await.unwrap();
        let elapsed = start.elapsed();

        assert!(
            elapsed >= delay,
            "Expected delay of at least {:?}, but got {:?}",
            delay,
            elapsed
        );
        assert!(
            elapsed < delay + Duration::from_millis(100),
            "Delay took too long: {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_full_capture_flow() {
        let backend = MockBackend::new();

        // Step 1: List windows
        let windows = backend.list_windows().await.unwrap();
        assert_eq!(windows.len(), 3);

        // Step 2: Resolve target
        let selector = WindowSelector::by_title("Firefox");
        let handle = backend.resolve_target(&selector).await.unwrap();
        assert_eq!(handle, "mock-0x1");

        // Step 3: Capture window
        let opts = CaptureOptions::builder()
            .format(ImageFormat::Png)
            .scale(0.5)
            .build();
        let image = backend.capture_window(handle, &opts).await.unwrap();
        assert_eq!(image.dimensions(), (960, 540));
    }

    #[tokio::test]
    async fn test_error_injection_flow() {
        let error = CaptureError::PermissionDenied {
            platform: "test".to_string(),
            backend:  BackendType::None,
        };
        let backend = MockBackend::new().with_error(error);

        // All operations should fail with the injected error
        assert!(backend.list_windows().await.is_err());

        let selector = WindowSelector::by_title("Firefox");
        assert!(backend.resolve_target(&selector).await.is_err());

        let handle = "mock-0x1".to_string();
        let opts = CaptureOptions::default();
        assert!(backend.capture_window(handle, &opts).await.is_err());
        assert!(backend.capture_display(None, &opts).await.is_err());
    }

    #[tokio::test]
    async fn test_capture_performance() {
        let backend = MockBackend::new();

        let start = Instant::now();

        // Full capture flow
        let windows = backend.list_windows().await.unwrap();
        assert_eq!(windows.len(), 3);

        let selector = WindowSelector::by_title("Firefox");
        let handle = backend.resolve_target(&selector).await.unwrap();

        let opts = CaptureOptions::default();
        let image = backend.capture_window(handle, &opts).await.unwrap();
        assert_eq!(image.dimensions(), (1920, 1080));

        let elapsed = start.elapsed();

        // Should complete in less than 2 seconds (should be much faster without delay)
        assert!(elapsed < Duration::from_secs(2), "Capture flow took too long: {:?}", elapsed);
    }
}
