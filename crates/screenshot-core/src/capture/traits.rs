//! Composable capability traits for screenshot capture backends
//!
//! This module defines fine-grained traits that backends can implement
//! based on their actual capabilities. Each trait represents a specific
//! capability, allowing backends to implement only what they support.
//!
//! # Trait Hierarchy
//!
//! - [`WindowEnumerator`]: List capturable windows (X11, Windows, not Wayland)
//! - [`WindowResolver`]: Resolve window selectors to handles
//! - [`ScreenCapture`]: Capture screenshots from windows/displays
//! - [`WaylandRestoreCapable`]: Wayland-specific restore token workflow
//! - [`BackendCapabilities`]: Query backend feature support

use async_trait::async_trait;

use crate::error::CaptureResult;
use crate::model::{CaptureOptions, SourceType, WindowHandle, WindowInfo, WindowSelector};

use super::ImageBuffer;

// ============================================================================
// Core Capability Traits
// ============================================================================

/// Capability: Backend can enumerate windows on the system.
///
/// # Platform Support
///
/// - **X11**: Full support via EWMH `_NET_CLIENT_LIST`
/// - **Windows**: Full support via `EnumWindows` API
/// - **Wayland**: Not supported (security model prevents enumeration)
///
/// Backends that cannot enumerate windows should not implement this trait.
/// Consumers can check for this capability via `Option<Arc<dyn WindowEnumerator>>`.
#[async_trait]
pub trait WindowEnumerator: Send + Sync {
    /// Lists all capturable windows on the system.
    ///
    /// Returns metadata about each window including title, class, process info.
    /// The returned handles can be used with [`WindowResolver`] or directly
    /// with [`ScreenCapture::capture_window`].
    async fn list_windows(&self) -> CaptureResult<Vec<WindowInfo>>;
}

/// Capability: Backend can resolve window selectors to handles.
///
/// Window selectors allow flexible window targeting by title (regex/substring),
/// window class, or executable name.
///
/// # Platform Support
///
/// - **X11**: Full support with title/class/exe matching
/// - **Windows**: Full support with title/class/exe matching
/// - **Wayland**: Partial support - resolves `wayland:` prefixed source IDs
#[async_trait]
pub trait WindowResolver: Send + Sync {
    /// Resolves a window selector to a concrete handle.
    ///
    /// The selector can match on:
    /// - `title_substring_or_regex`: Window title (substring or regex pattern)
    /// - `class`: Window class (X11 WM_CLASS, Windows class name)
    /// - `exe`: Process executable name
    ///
    /// All non-None criteria must match (AND semantics).
    async fn resolve(&self, selector: &WindowSelector) -> CaptureResult<WindowHandle>;
}

/// Capability: Backend can capture screenshots.
///
/// This is the core capture trait that all backends must implement.
/// It provides methods for capturing specific windows or entire displays.
#[async_trait]
pub trait ScreenCapture: Send + Sync {
    /// Captures a screenshot of a specific window.
    ///
    /// # Arguments
    ///
    /// * `handle` - Window handle obtained from [`WindowEnumerator`] or [`WindowResolver`]
    /// * `opts` - Capture options (format, quality, region, cursor, etc.)
    ///
    /// # Platform Notes
    ///
    /// - **Wayland**: Requires a valid restore token for the handle
    /// - **X11/Windows**: Direct capture via window ID
    async fn capture_window(
        &self,
        handle: WindowHandle,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer>;

    /// Captures a screenshot of a display/monitor.
    ///
    /// # Arguments
    ///
    /// * `display_id` - Optional display ID. If `None`, captures the primary display.
    /// * `opts` - Capture options (format, quality, region, cursor, etc.)
    async fn capture_display(
        &self,
        display_id: Option<u32>,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer>;
}

// ============================================================================
// Platform-Specific Traits
// ============================================================================

/// Result of a Wayland consent priming operation.
///
/// Contains the source IDs where restore tokens were stored and metadata
/// about the consent session.
#[derive(Debug, Clone)]
pub struct PrimeConsentResult {
    /// Primary source ID (for single stream or first of multiple)
    pub primary_source_id: String,
    /// All source IDs (includes primary)
    pub all_source_ids: Vec<String>,
    /// Number of streams/sources captured
    pub num_streams: usize,
}

/// Capability: Backend supports Wayland restore token workflow.
///
/// Wayland's security model prevents window enumeration and requires
/// user consent via XDG Desktop Portal. This trait provides methods
/// for the restore token workflow:
///
/// 1. `prime_consent()` - Show portal picker, store restore token
/// 2. `capture_with_token()` - Use stored token for headless capture
///
/// # Platform Support
///
/// - **Wayland**: Required for window capture
/// - **X11/Windows**: Not applicable
#[async_trait]
pub trait WaylandRestoreCapable: Send + Sync {
    /// Primes user consent and stores a restore token.
    ///
    /// This shows the XDG Desktop Portal picker, allowing the user to
    /// select a window or display. The consent is stored as a restore
    /// token that can be used for subsequent headless captures.
    ///
    /// # Arguments
    ///
    /// * `source_type` - Type of source to capture (Window or Display)
    /// * `source_id` - User-provided identifier for storing the token
    /// * `include_cursor` - Whether to include cursor in captures
    ///
    /// # Returns
    ///
    /// Details about the stored consent including the source ID.
    async fn prime_consent(
        &self,
        source_type: SourceType,
        source_id: &str,
        include_cursor: bool,
    ) -> CaptureResult<PrimeConsentResult>;

    /// Captures using a stored restore token.
    ///
    /// This performs a headless capture using a previously stored
    /// restore token, without requiring user interaction.
    ///
    /// # Arguments
    ///
    /// * `source_id` - The source ID used when priming consent
    /// * `opts` - Capture options
    async fn capture_with_token(
        &self,
        source_id: &str,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer>;

    /// Lists all stored source IDs with active restore tokens.
    fn list_sources(&self) -> CaptureResult<Vec<String>>;
}

// ============================================================================
// Capability Query Trait
// ============================================================================

/// Query backend feature support at runtime.
///
/// This complements the type-level capability traits by providing
/// runtime queryable feature flags. Useful for serialization to
/// MCP tool responses and conditional feature enablement.
pub trait BackendCapabilities: Send + Sync {
    /// Whether the backend supports including cursor in captures.
    fn supports_cursor(&self) -> bool;

    /// Whether the backend supports region/crop capture.
    fn supports_region(&self) -> bool;

    /// Whether the backend supports Wayland restore tokens.
    fn supports_wayland_restore(&self) -> bool {
        false
    }

    /// Whether the backend supports window enumeration (listing available windows).
    fn supports_window_enumeration(&self) -> bool {
        true
    }

    /// Whether the backend can capture displays.
    fn supports_display_capture(&self) -> bool {
        true
    }
}
