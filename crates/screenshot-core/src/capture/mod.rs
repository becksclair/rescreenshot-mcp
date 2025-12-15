//! Capture backend traits and implementations
//!
//! This module provides the core abstractions for screenshot capture across
//! different platforms using composable capability traits.
//!
//! # Architecture
//!
//! The capture system uses a capability-based design where backends implement
//! only the traits they support:
//!
//! - [`WindowEnumerator`] - List all capturable windows (X11, Windows)
//! - [`WindowResolver`] - Resolve window selectors to handles (all backends)
//! - [`ScreenCapture`] - Capture screenshots (all backends)
//! - [`WaylandRestoreCapable`] - Wayland restore token workflow (Wayland only)
//!
//! ## Recommended Usage
//!
//! Use [`create_default_backend()`] which returns [`CompositeBackend`] with
//! typed access to capabilities:
//!
//! ```rust,ignore
//! use screenshot_core::capture::{create_default_backend, ScreenCapture};
//!
//! let backend = create_default_backend()?;
//!
//! // Type-safe capability access (no downcasting needed!)
//! if let Some(wayland) = &backend.wayland_restore {
//!     wayland.prime_consent(source_type, source_id, false).await?;
//! }
//!
//! // Direct capture via typed field
//! let image = backend.capture.capture_display(None, &opts).await?;
//! ```
//!
//! ## Backend Capabilities
//!
//! | Backend | WindowEnumerator | WindowResolver | ScreenCapture | WaylandRestore |
//! |---------|------------------|----------------|---------------|----------------|
//! | Windows | ✓ | ✓ | ✓ | - |
//! | X11     | ✓ | ✓ | ✓ | - |
//! | Wayland | - | ✓ | ✓ | ✓ |
//! | Mock    | ✓ | ✓ | ✓ | - |
//!
//! # Core Types
//!
//! - [`CompositeBackend`] - Facade with typed capability accessors
//! - [`ImageBuffer`] - Image wrapper with scale/crop transformations
//! - [`Capabilities`](crate::model::Capabilities) - Runtime capability flags

use std::sync::Arc;

use crate::error::CaptureResult;

pub mod composite;
pub mod constants;
pub mod image_buffer;
pub mod matching;
pub mod mock;
pub mod traits;

#[cfg(target_os = "linux")]
pub mod wayland_backend;

#[cfg(target_os = "linux")]
pub mod x11_backend;

#[cfg(target_os = "windows")]
pub mod windows_backend;

pub use composite::CompositeBackend;
pub use composite::composite_from_mock;
#[cfg(target_os = "windows")]
pub use composite::composite_from_windows;
#[cfg(target_os = "linux")]
pub use composite::{composite_from_wayland, composite_from_x11};
pub use image_buffer::ImageBuffer;
pub use matching::WindowMatcher;
pub use mock::MockBackend;
pub use traits::{
    BackendCapabilities, PrimeConsentResult, ScreenCapture, WaylandRestoreCapable,
    WindowEnumerator, WindowResolver,
};
#[cfg(target_os = "linux")]
pub use wayland_backend::WaylandBackend;
#[cfg(target_os = "windows")]
pub use windows_backend::WindowsBackend;
#[cfg(target_os = "linux")]
pub use x11_backend::X11Backend;

/// Creates a default capture backend for the current platform.
///
/// Returns a [`CompositeBackend`] that provides type-safe access to backend
/// capabilities without runtime downcasting. Use the typed fields (`enumerator`,
/// `resolver`, `capture`, `wayland_restore`) to access specific capabilities.
///
/// - **Windows**: Uses `WindowsBackend` with all capabilities
/// - **Linux/Wayland**: Uses `WaylandBackend` (no window enumeration, has restore tokens)
/// - **Linux/X11**: Uses `X11Backend` (full window enumeration)
/// - **macOS/Other**: Returns a structured `BackendNotAvailable` error
pub fn create_default_backend() -> CaptureResult<Arc<CompositeBackend>> {
    #[cfg(target_os = "windows")]
    {
        let backend = Arc::new(WindowsBackend::new()?);
        Ok(Arc::new(composite_from_windows(backend)))
    }

    #[cfg(target_os = "linux")]
    {
        use crate::{error::CaptureError, model::BackendType};

        let platform = crate::util::detect::detect_platform();

        match platform.backend {
            BackendType::Wayland => {
                let key_store = Arc::new(crate::util::key_store::KeyStore::new());
                let backend = Arc::new(WaylandBackend::new(key_store));
                Ok(Arc::new(composite_from_wayland(backend)))
            }
            BackendType::X11 => {
                let backend = Arc::new(X11Backend::new()?);
                Ok(Arc::new(composite_from_x11(backend)))
            }
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
