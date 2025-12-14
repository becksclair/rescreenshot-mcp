//! Capture backend traits and implementations
//!
//! This module provides the core abstractions for screenshot capture across
//! different platforms. It includes:
//!
//! - `ImageBuffer`: A wrapper around `image::DynamicImage` with transformation
//!   methods for scaling, cropping, and format conversion
//! - `CaptureFacade`: Trait defining the interface for capture backends
//! - Backend implementations for Wayland, X11, Windows, and macOS (to be
//!   implemented in future phases)

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    error::CaptureResult,
    model::{
        Capabilities, CaptureOptions, CaptureSource, WindowHandle, WindowInfo, WindowSelector,
    },
};

#[cfg(target_os = "linux")]
use crate::error::CaptureError;

pub mod image_buffer;
pub mod matching;
pub mod mock;

#[cfg(target_os = "linux")]
pub mod wayland_backend;

#[cfg(target_os = "linux")]
pub mod x11_backend;

#[cfg(target_os = "windows")]
pub mod windows_backend;

pub use image_buffer::ImageBuffer;
pub use matching::WindowMatcher;
pub use mock::MockBackend;
#[cfg(target_os = "linux")]
pub use wayland_backend::{PrimeConsentResult, WaylandBackend};
#[cfg(target_os = "windows")]
pub use windows_backend::WindowsBackend;
#[cfg(target_os = "linux")]
pub use x11_backend::X11Backend;

#[cfg(target_os = "linux")]
struct LinuxAutoBackend {
    primary: Arc<dyn CaptureFacade>,
    fallback: Option<Arc<dyn CaptureFacade>>,
}

#[cfg(target_os = "linux")]
impl LinuxAutoBackend {
    fn new(primary: Arc<dyn CaptureFacade>, fallback: Option<Arc<dyn CaptureFacade>>) -> Self {
        Self { primary, fallback }
    }

    fn union_capabilities(&self) -> Capabilities {
        let primary = self.primary.capabilities();
        let Some(fallback) = &self.fallback else {
            return primary;
        };

        let fb = fallback.capabilities();
        Capabilities {
            supports_cursor: primary.supports_cursor || fb.supports_cursor,
            supports_region: primary.supports_region || fb.supports_region,
            supports_wayland_restore: primary.supports_wayland_restore
                || fb.supports_wayland_restore,
            supports_window_capture: primary.supports_window_capture || fb.supports_window_capture,
            supports_display_capture: primary.supports_display_capture
                || fb.supports_display_capture,
        }
    }

    fn should_fallback_on_error(err: &CaptureError) -> bool {
        matches!(
            err,
            CaptureError::PortalUnavailable { .. }
                | CaptureError::BackendNotAvailable { .. }
                | CaptureError::CaptureTimeout { .. }
                | CaptureError::WindowNotFound { .. }
                | CaptureError::TokenNotFound { .. }
        )
    }

    fn selector_is_explicit_wayland(selector: &WindowSelector) -> bool {
        selector
            .exe
            .as_deref()
            .is_some_and(|exe| exe.starts_with("wayland:"))
    }
}

#[cfg(target_os = "linux")]
#[async_trait]
impl CaptureFacade for LinuxAutoBackend {
    async fn list_windows(&self) -> CaptureResult<Vec<WindowInfo>> {
        // Best-effort: show both Wayland primed sources and X11 windows when available.
        let mut windows = self.primary.list_windows().await?;

        if let Some(fallback) = &self.fallback {
            if let Ok(mut more) = fallback.list_windows().await {
                windows.append(&mut more);
            }
        }

        Ok(windows)
    }

    async fn resolve_target(&self, selector: &WindowSelector) -> CaptureResult<WindowHandle> {
        // If user is explicitly targeting a Wayland source, go straight to the primary
        // backend so we don't "helpfully" match an X11 window named similarly.
        if Self::selector_is_explicit_wayland(selector) {
            return self.primary.resolve_target(selector).await;
        }

        if let Some(fallback) = &self.fallback {
            match fallback.resolve_target(selector).await {
                Ok(handle) => return Ok(handle),
                Err(e) if !Self::should_fallback_on_error(&e) => return Err(e),
                Err(_) => {
                    // Fall through to primary.
                }
            }
        }

        self.primary.resolve_target(selector).await
    }

    async fn capture_window(
        &self,
        handle: WindowHandle,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        match self.primary.capture_window(handle.clone(), opts).await {
            Ok(img) => Ok(img),
            Err(e) => {
                if let Some(fallback) = &self.fallback {
                    if Self::should_fallback_on_error(&e) {
                        return fallback.capture_window(handle, opts).await;
                    }
                }
                Err(e)
            }
        }
    }

    async fn capture_display(
        &self,
        display_id: Option<u32>,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        match self.primary.capture_display(display_id, opts).await {
            Ok(img) => Ok(img),
            Err(e) => {
                if let Some(fallback) = &self.fallback {
                    if Self::should_fallback_on_error(&e) {
                        return fallback.capture_display(display_id, opts).await;
                    }
                }
                Err(e)
            }
        }
    }

    fn capabilities(&self) -> Capabilities {
        self.union_capabilities()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        // Delegate downcasting to the currently selected primary backend so tools
        // like `prime_wayland_consent` can downcast to `WaylandBackend`.
        self.primary.as_any()
    }
}

/// Creates a default capture backend for the current platform.
///
/// The default build compiles only native platform dependencies and selects a
/// working backend automatically at runtime.
///
/// - **Windows**: Uses `WindowsBackend`
/// - **Linux**: Prefers Wayland when detected, but may fall back to X11 when
///   available and Wayland operations fail.
/// - **macOS/Other**: Returns a structured `BackendNotAvailable` error
pub fn create_default_backend() -> CaptureResult<Arc<dyn CaptureFacade>> {
    #[cfg(target_os = "windows")]
    {
        Ok(Arc::new(WindowsBackend::new()?))
    }

    #[cfg(target_os = "linux")]
    {
        use crate::{error::CaptureError, model::BackendType};

        let platform = crate::util::detect::detect_platform();

        match platform.backend {
            BackendType::Wayland => {
                let key_store = Arc::new(crate::util::key_store::KeyStore::new());
                let primary: Arc<dyn CaptureFacade> = Arc::new(WaylandBackend::new(key_store));

                // If X11 is available in this session (e.g., XWayland), keep it as a fallback.
                let fallback: Option<Arc<dyn CaptureFacade>> = X11Backend::new()
                    .ok()
                    .map(|b| Arc::new(b) as Arc<dyn CaptureFacade>);

                Ok(if fallback.is_some() {
                    Arc::new(LinuxAutoBackend::new(primary, fallback))
                } else {
                    primary
                })
            }
            BackendType::X11 => Ok(Arc::new(X11Backend::new()?)),
            BackendType::None | BackendType::Windows | BackendType::MacOS => {
                Err(CaptureError::BackendNotAvailable {
                    backend: BackendType::None,
                })
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        Err(crate::error::CaptureError::BackendNotAvailable {
            backend: crate::model::BackendType::MacOS,
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(crate::error::CaptureError::BackendNotAvailable {
            backend: crate::model::BackendType::None,
        })
    }
}

/// Core trait for screenshot capture backends
///
/// `CaptureFacade` defines the interface that all platform-specific screenshot
/// backends must implement. This trait enables pluggable backends for different
/// platforms (Wayland, X11, Windows, macOS) while maintaining a consistent API.
///
/// All implementations must be thread-safe (`Send + Sync`) to support
/// concurrent capture operations in async contexts.
///
/// # Methods
///
/// - [`list_windows`](CaptureFacade::list_windows) - Enumerate all capturable
///   windows
/// - [`resolve_target`](CaptureFacade::resolve_target) - Find window by
///   selector
/// - [`capture_window`](CaptureFacade::capture_window) - Capture a specific
///   window
/// - [`capture_display`](CaptureFacade::capture_display) - Capture full display
/// - [`capabilities`](CaptureFacade::capabilities) - Query backend capabilities
///
/// # Thread Safety
///
/// All methods can be called concurrently from multiple tasks. Implementations
/// should use internal synchronization if needed to protect shared state.
///
/// # Examples
///
/// ## Implementing a custom backend
///
/// ```rust,ignore
/// use async_trait::async_trait;
/// use screenshot_mcp::{
///     capture::{CaptureFacade, ImageBuffer},
///     error::CaptureResult,
///     model::*,
/// };
///
/// struct MyBackend {
///     // Backend-specific state
/// }
///
/// #[async_trait]
/// impl CaptureFacade for MyBackend {
///     async fn list_windows(&self) -> CaptureResult<Vec<WindowInfo>> {
///         // Platform-specific window enumeration
///         Ok(vec![])
///     }
///
///     async fn resolve_target(
///         &self,
///         selector: &WindowSelector,
///     ) -> CaptureResult<WindowHandle> {
///         // Platform-specific window lookup
///         Ok("window-id".to_string())
///     }
///
///     async fn capture_window(
///         &self,
///         handle: WindowHandle,
///         opts: &CaptureOptions,
///     ) -> CaptureResult<ImageBuffer> {
///         // Platform-specific window capture
///         Ok(ImageBuffer::from_test_pattern(1920, 1080))
///     }
///
///     async fn capture_display(
///         &self,
///         display_id: Option<u32>,
///         opts: &CaptureOptions,
///     ) -> CaptureResult<ImageBuffer> {
///         // Platform-specific display capture
///         Ok(ImageBuffer::from_test_pattern(1920, 1080))
///     }
///
///     fn capabilities(&self) -> Capabilities {
///         // Return what this backend supports
///         Capabilities::full()
///     }
/// }
/// ```
///
/// ## Using a backend
///
/// ```rust,ignore
/// use screenshot_mcp::{
///     capture::CaptureFacade,
///     model::{CaptureOptions, WindowSelector},
/// };
///
/// async fn capture_firefox(backend: &dyn CaptureFacade) -> Result<(), Box<dyn
/// std::error::Error>> {     // Find Firefox window
///     let selector = WindowSelector::by_title("Firefox");
///     let handle = backend.resolve_target(&selector).await?;
///
///     // Capture with default options
///     let opts = CaptureOptions::default();
///     let image = backend.capture_window(handle, &opts).await?;
///
///     println!("Captured {}x{} image", image.dimensions().0,
/// image.dimensions().1);     Ok(())
/// }
/// ```
#[async_trait]
pub trait CaptureFacade: Send + Sync {
    /// Lists all capturable windows on the system
    ///
    /// Returns metadata about windows that can be captured, including
    /// their IDs, titles, classes, and owning processes. The exact
    /// information available depends on the backend capabilities.
    ///
    /// # Returns
    ///
    /// A vector of [`WindowInfo`] structs describing available windows.
    /// The vector may be empty if no windows are available or the backend
    /// doesn't support window enumeration.
    ///
    /// # Errors
    ///
    /// - [`CaptureError::BackendNotAvailable`](crate::error::CaptureError::BackendNotAvailable)
    ///   - Backend cannot enumerate windows
    /// - [`CaptureError::PermissionDenied`](crate::error::CaptureError::PermissionDenied)
    ///   - System denies access to window list
    /// - [`CaptureError::CaptureTimeout`](crate::error::CaptureError::CaptureTimeout)
    ///   - Operation took too long
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use screenshot_mcp::capture::CaptureFacade;
    ///
    /// async fn show_windows(backend: &dyn CaptureFacade) {
    ///     match backend.list_windows().await {
    ///         Ok(windows) => {
    ///             println!("Found {} windows:", windows.len());
    ///             for win in windows {
    ///                 println!("  - {} ({})", win.title, win.class);
    ///             }
    ///         }
    ///         Err(e) => eprintln!("Error: {}", e),
    ///     }
    /// }
    /// ```
    async fn list_windows(&self) -> CaptureResult<Vec<WindowInfo>>;

    /// Resolves a window selector to a specific window handle
    ///
    /// Searches for a window matching the given selector criteria (title,
    /// class, or executable name). If multiple windows match, the first
    /// match is returned (order is backend-specific).
    ///
    /// # Arguments
    ///
    /// - `selector` - Criteria for finding the window (title, class, or exe)
    ///
    /// # Returns
    ///
    /// A [`WindowHandle`] that can be used with
    /// [`capture_window`](CaptureFacade::capture_window).
    ///
    /// # Errors
    ///
    /// - [`CaptureError::WindowNotFound`](crate::error::CaptureError::WindowNotFound)
    ///   - No window matches the selector
    /// - [`CaptureError::BackendNotAvailable`](crate::error::CaptureError::BackendNotAvailable)
    ///   - Backend cannot resolve windows
    /// - [`CaptureError::PermissionDenied`](crate::error::CaptureError::PermissionDenied)
    ///   - System denies access to window information
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use screenshot_mcp::{capture::CaptureFacade, model::WindowSelector};
    ///
    /// async fn find_browser(backend: &dyn CaptureFacade) -> Result<String,
    /// Box<dyn std::error::Error>> {     // Try Firefox first
    ///     let selector = WindowSelector::by_title("Firefox");
    ///     if let Ok(handle) = backend.resolve_target(&selector).await {
    ///         return Ok(handle);
    ///     }
    ///
    ///     // Fall back to Chrome
    ///     let selector = WindowSelector::by_title("Chrome");
    ///     Ok(backend.resolve_target(&selector).await?)
    /// }
    /// ```
    async fn resolve_target(&self, selector: &WindowSelector) -> CaptureResult<WindowHandle>;

    /// Captures a screenshot of a specific window
    ///
    /// Takes a screenshot of the window identified by the handle. The
    /// captured image respects the options provided (format, quality,
    /// scaling, region, etc.).
    ///
    /// # Arguments
    ///
    /// - `handle` - Window identifier from
    ///   [`resolve_target`](CaptureFacade::resolve_target)
    /// - `opts` - Capture options (format, quality, scale, region, cursor)
    ///
    /// # Returns
    ///
    /// An [`ImageBuffer`] containing the captured screenshot.
    ///
    /// # Errors
    ///
    /// - [`CaptureError::WindowNotFound`](crate::error::CaptureError::WindowNotFound)
    ///   - Window no longer exists
    /// - [`CaptureError::PermissionDenied`](crate::error::CaptureError::PermissionDenied)
    ///   - User denied screenshot permission
    /// - [`CaptureError::BackendNotAvailable`](crate::error::CaptureError::BackendNotAvailable)
    ///   - Backend cannot capture windows
    /// - [`CaptureError::CaptureTimeout`](crate::error::CaptureError::CaptureTimeout)
    ///   - Operation took too long (e.g., waiting for permission dialog)
    /// - [`CaptureError::InvalidParameter`](crate::error::CaptureError::InvalidParameter)
    ///   - Invalid capture options (e.g., region out of bounds)
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use screenshot_mcp::{
    ///     capture::CaptureFacade,
    ///     model::{CaptureOptions, ImageFormat, WindowSelector},
    /// };
    ///
    /// async fn capture_window_as_webp(
    ///     backend: &dyn CaptureFacade,
    ///     title: &str,
    /// ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    ///     // Find window
    ///     let selector = WindowSelector::by_title(title);
    ///     let handle = backend.resolve_target(&selector).await?;
    ///
    ///     // Capture with high quality WebP
    ///     let opts = CaptureOptions::builder()
    ///         .format(ImageFormat::Webp)
    ///         .quality(90)
    ///         .build();
    ///
    ///     let image = backend.capture_window(handle, &opts).await?;
    ///     Ok(image.as_bytes().to_vec())
    /// }
    /// ```
    async fn capture_window(
        &self,
        handle: WindowHandle,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer>;

    /// Captures a screenshot of an entire display
    ///
    /// Takes a screenshot of a full display/monitor. If `display_id` is
    /// `None`, captures the primary display. The captured image respects
    /// the options provided (format, quality, scaling, region, etc.).
    ///
    /// # Arguments
    ///
    /// - `display_id` - Display identifier (`None` for primary display)
    /// - `opts` - Capture options (format, quality, scale, region, cursor)
    ///
    /// # Returns
    ///
    /// An [`ImageBuffer`] containing the captured screenshot.
    ///
    /// # Errors
    ///
    /// - [`CaptureError::PermissionDenied`](crate::error::CaptureError::PermissionDenied)
    ///   - User denied screenshot permission
    /// - [`CaptureError::BackendNotAvailable`](crate::error::CaptureError::BackendNotAvailable)
    ///   - Backend cannot capture displays
    /// - [`CaptureError::CaptureTimeout`](crate::error::CaptureError::CaptureTimeout)
    ///   - Operation took too long (e.g., waiting for permission dialog)
    /// - [`CaptureError::InvalidParameter`](crate::error::CaptureError::InvalidParameter)
    ///   - Invalid display ID or capture options
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use screenshot_mcp::{
    ///     capture::CaptureFacade,
    ///     model::{CaptureOptions, ImageFormat},
    /// };
    ///
    /// async fn capture_primary_display(
    ///     backend: &dyn CaptureFacade,
    /// ) -> Result<(), Box<dyn std::error::Error>> {
    ///     // Capture primary display as PNG
    ///     let opts = CaptureOptions::default();
    ///     let image = backend.capture_display(None, &opts).await?;
    ///
    ///     println!("Captured {}x{} screenshot", image.dimensions().0,
    /// image.dimensions().1);     Ok(())
    /// }
    ///
    /// async fn capture_secondary_display(
    ///     backend: &dyn CaptureFacade,
    /// ) -> Result<(), Box<dyn std::error::Error>> {
    ///     // Capture display 1 (second monitor)
    ///     let opts = CaptureOptions::default();
    ///     let image = backend.capture_display(Some(1), &opts).await?;
    ///     Ok(())
    /// }
    /// ```
    async fn capture_display(
        &self,
        display_id: Option<u32>,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer>;

    /// Unified capture entry point for all source types
    ///
    /// This is the primary method for capturing screenshots. It accepts a
    /// [`CaptureSource`] enum that can represent windows, displays, or regions,
    /// providing a single, extensible API for all capture operations.
    ///
    /// # Arguments
    ///
    /// - `source` - The capture source (window, display, or region)
    /// - `opts` - Capture options (format, quality, scale, region, cursor)
    ///
    /// # Returns
    ///
    /// An [`ImageBuffer`] containing the captured screenshot.
    ///
    /// # Errors
    ///
    /// See [`capture_window`](CaptureFacade::capture_window) and
    /// [`capture_display`](CaptureFacade::capture_display) for error details.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use screenshot_core::{
    ///     capture::CaptureFacade,
    ///     model::{CaptureOptions, CaptureSource},
    /// };
    ///
    /// async fn capture_example(backend: &dyn CaptureFacade) -> Result<(), Box<dyn std::error::Error>> {
    ///     let opts = CaptureOptions::default();
    ///
    ///     // Capture a window
    ///     let window_source = CaptureSource::Window("12345".to_string());
    ///     let window_img = backend.capture(window_source, &opts).await?;
    ///
    ///     // Capture primary display
    ///     let display_source = CaptureSource::Display(None);
    ///     let display_img = backend.capture(display_source, &opts).await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    async fn capture(
        &self,
        source: CaptureSource,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        match source {
            CaptureSource::Window(handle) => self.capture_window(handle, opts).await,
            CaptureSource::Display(display_id) => self.capture_display(display_id, opts).await,
            CaptureSource::Region(region) => {
                // For now, region capture is implemented as post-processing on display capture
                // Future backends may support native region capture
                let display_img = self.capture_display(None, opts).await?;
                display_img.crop(region)
            }
        }
    }

    /// Returns the capabilities of this backend
    ///
    /// Describes which features are supported by this backend. Different
    /// backends have different capabilities (e.g., Wayland supports
    /// restore tokens, X11 doesn't).
    ///
    /// This is a synchronous method since capabilities are typically known
    /// at backend initialization and don't require I/O or async operations.
    ///
    /// # Returns
    ///
    /// A [`Capabilities`] struct describing supported features.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use screenshot_mcp::capture::CaptureFacade;
    ///
    /// fn check_features(backend: &dyn CaptureFacade) {
    ///     let caps = backend.capabilities();
    ///
    ///     if caps.supports_window_capture {
    ///         println!("Backend supports window capture");
    ///     }
    ///
    ///     if caps.supports_wayland_restore {
    ///         println!("Backend supports permission-free recapture with restore
    /// tokens");     }
    ///
    ///     if !caps.supports_cursor {
    ///         println!("Warning: Backend cannot include cursor in screenshots");
    ///     }
    /// }
    /// ```
    fn capabilities(&self) -> Capabilities;

    /// Enables downcasting to concrete backend types
    ///
    /// This method allows platform-specific MCP tools (like
    /// `prime_wayland_consent`) to safely downcast from `dyn CaptureFacade`
    /// to concrete backend types (e.g., `WaylandBackend`).
    ///
    /// # Returns
    ///
    /// A reference to `self` as `&dyn std::any::Any`, which can be used
    /// with `.downcast_ref::<ConcreteType>()`.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use screenshot_mcp::capture::{CaptureFacade, WaylandBackend};
    ///
    /// fn use_wayland_specific_feature(backend: &dyn CaptureFacade) {
    ///     if let Some(wayland) = backend.as_any().downcast_ref::<WaylandBackend>() {
    ///         // Use Wayland-specific methods
    ///         wayland.prime_consent(...);
    ///     } else {
    ///         eprintln!("This feature requires Wayland backend");
    ///     }
    /// }
    /// ```
    fn as_any(&self) -> &dyn std::any::Any;
}
