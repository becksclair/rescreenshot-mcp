//! Composite backend that holds optional capability trait objects
//!
//! This module provides a facade that wraps backends with type-safe
//! capability access. Instead of using `as_any()` downcasting, consumers
//! can check for capabilities via typed optional fields.
//!
//! # Example
//!
//! ```rust,ignore
//! use screenshot_core::capture::CompositeBackend;
//!
//! let backend = create_composite_backend()?;
//!
//! // Type-safe capability check
//! if let Some(enumerator) = &backend.enumerator {
//!     let windows = enumerator.list_windows().await?;
//! }
//!
//! // Wayland-specific access without downcasting
//! if let Some(wayland) = &backend.wayland_restore {
//!     let result = wayland.prime_consent(source_type, source_id, cursor).await?;
//! }
//! ```

use std::sync::Arc;

use super::{
    BackendCapabilities, ScreenCapture, WaylandRestoreCapable, WindowEnumerator, WindowResolver,
};
use crate::model::Capabilities;

/// Composite backend holding optional capability trait objects.
///
/// This provides type-safe access to backend capabilities without
/// runtime downcasting. Each capability is an optional trait object
/// that can be checked at compile time.
///
/// # Capabilities
///
/// - `enumerator`: Window enumeration (X11, Windows - not Wayland)
/// - `resolver`: Window selector resolution (all backends)
/// - `capture`: Screenshot capture (all backends)
/// - `wayland_restore`: Wayland restore token workflow (Wayland only)
pub struct CompositeBackend {
    /// Window enumeration capability.
    ///
    /// Present on X11 and Windows backends.
    /// Not present on Wayland (security model prevents enumeration).
    pub enumerator: Option<Arc<dyn WindowEnumerator>>,

    /// Window selector resolution capability.
    ///
    /// Present on all backends. Wayland resolves via restore token lookup.
    pub resolver: Option<Arc<dyn WindowResolver>>,

    /// Screen capture capability.
    ///
    /// Present on all backends. This is the core capture functionality.
    pub capture: Arc<dyn ScreenCapture>,

    /// Wayland restore token capability.
    ///
    /// Only present on Wayland backend. Enables headless capture
    /// after initial consent via `prime_consent()`.
    pub wayland_restore: Option<Arc<dyn WaylandRestoreCapable>>,

    /// Capability flags for runtime queries.
    pub capabilities: Capabilities,

    /// Backend name for diagnostics.
    pub name: &'static str,
}

impl CompositeBackend {
    /// Creates a new CompositeBackend with the specified capabilities.
    pub fn new(
        enumerator: Option<Arc<dyn WindowEnumerator>>,
        resolver: Option<Arc<dyn WindowResolver>>,
        capture: Arc<dyn ScreenCapture>,
        wayland_restore: Option<Arc<dyn WaylandRestoreCapable>>,
        capabilities: Capabilities,
        name: &'static str,
    ) -> Self {
        Self {
            enumerator,
            resolver,
            capture,
            wayland_restore,
            capabilities,
            name,
        }
    }

    /// Returns true if window enumeration is available.
    pub fn has_window_enumeration(&self) -> bool {
        self.enumerator.is_some()
    }

    /// Returns true if window resolution is available.
    pub fn has_window_resolver(&self) -> bool {
        self.resolver.is_some()
    }

    /// Returns true if Wayland restore tokens are supported.
    pub fn has_wayland_restore(&self) -> bool {
        self.wayland_restore.is_some()
    }

    /// Returns the Wayland-specific capability if available.
    pub fn wayland(&self) -> Option<&Arc<dyn WaylandRestoreCapable>> {
        self.wayland_restore.as_ref()
    }
}

impl std::fmt::Debug for CompositeBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeBackend")
            .field("name", &self.name)
            .field("has_enumerator", &self.enumerator.is_some())
            .field("has_resolver", &self.resolver.is_some())
            .field("has_wayland_restore", &self.wayland_restore.is_some())
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

impl BackendCapabilities for CompositeBackend {
    fn supports_cursor(&self) -> bool {
        self.capabilities.supports_cursor
    }

    fn supports_region(&self) -> bool {
        self.capabilities.supports_region
    }

    fn supports_wayland_restore(&self) -> bool {
        self.capabilities.supports_wayland_restore
    }

    fn supports_window_enumeration(&self) -> bool {
        self.capabilities.supports_window_enumeration
    }

    fn supports_display_capture(&self) -> bool {
        self.capabilities.supports_display_capture
    }
}

// ============================================================================
// Factory Functions
// ============================================================================

/// Creates a CompositeBackend from a MockBackend.
///
/// Note: `wayland_restore` is set to `None` because MockBackend doesn't
/// implement `WaylandRestoreCapable`. To test Wayland restore workflows,
/// use a real WaylandBackend or create a separate MockWaylandBackend.
pub fn composite_from_mock(backend: Arc<super::MockBackend>) -> CompositeBackend {
    CompositeBackend::new(
        Some(backend.clone() as Arc<dyn WindowEnumerator>),
        Some(backend.clone() as Arc<dyn WindowResolver>),
        backend.clone() as Arc<dyn ScreenCapture>,
        None, // MockBackend doesn't implement WaylandRestoreCapable
        Capabilities::full(),
        "mock",
    )
}

/// Creates a CompositeBackend for Windows.
#[cfg(target_os = "windows")]
pub fn composite_from_windows(backend: Arc<super::WindowsBackend>) -> CompositeBackend {
    CompositeBackend::new(
        Some(backend.clone() as Arc<dyn WindowEnumerator>),
        Some(backend.clone() as Arc<dyn WindowResolver>),
        backend.clone() as Arc<dyn ScreenCapture>,
        None,
        Capabilities {
            supports_cursor: true,
            supports_region: true,
            supports_wayland_restore: false,
            supports_window_enumeration: true,
            supports_display_capture: true,
        },
        "windows",
    )
}

/// Creates a CompositeBackend for X11.
#[cfg(target_os = "linux")]
pub fn composite_from_x11(backend: Arc<super::X11Backend>) -> CompositeBackend {
    CompositeBackend::new(
        Some(backend.clone() as Arc<dyn WindowEnumerator>),
        Some(backend.clone() as Arc<dyn WindowResolver>),
        backend.clone() as Arc<dyn ScreenCapture>,
        None,
        Capabilities {
            supports_cursor: false,
            supports_region: true,
            supports_wayland_restore: false,
            supports_window_enumeration: true,
            supports_display_capture: true,
        },
        "x11",
    )
}

/// Creates a CompositeBackend for Wayland.
#[cfg(target_os = "linux")]
pub fn composite_from_wayland(backend: Arc<super::WaylandBackend>) -> CompositeBackend {
    CompositeBackend::new(
        None, // Wayland cannot enumerate windows
        Some(backend.clone() as Arc<dyn WindowResolver>),
        backend.clone() as Arc<dyn ScreenCapture>,
        Some(backend.clone() as Arc<dyn WaylandRestoreCapable>),
        Capabilities {
            supports_cursor: true,
            supports_region: true,
            supports_wayland_restore: true,
            supports_window_enumeration: false,
            supports_display_capture: true,
        },
        "wayland",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_composite_from_mock() {
        let mock = Arc::new(super::super::MockBackend::new());
        let composite = composite_from_mock(mock);

        assert!(composite.has_window_enumeration());
        assert!(composite.has_window_resolver());
        assert!(!composite.has_wayland_restore());
        assert_eq!(composite.name, "mock");

        // Test list_windows through enumerator capability
        let enumerator = composite.enumerator.as_ref().unwrap();
        let windows = enumerator.list_windows().await.unwrap();
        assert_eq!(windows.len(), 3);
    }

    #[tokio::test]
    async fn test_composite_capabilities() {
        let mock = Arc::new(super::super::MockBackend::new());
        let composite = composite_from_mock(mock);

        let caps = composite.capabilities;
        assert!(caps.supports_cursor);
        assert!(caps.supports_region);
        assert!(caps.supports_window_enumeration);
        assert!(caps.supports_display_capture);
    }
}
