//! Wayland capture backend using XDG Desktop Portal with restore tokens
//!
//! This module implements screenshot capture for Wayland compositors using the
//! XDG Desktop Portal ScreenCast API. It provides:
//!
//! - **Restore Token Support**: Permission-free recapture after initial consent
//! - **Automatic Token Rotation**: Single-use tokens are rotated after each
//!   capture
//! - **Graceful Fallback**: Falls back to display capture + region crop if
//!   restore fails
//! - **Timeout Protection**: All portal operations have 30-second timeouts
//!
//! # Architecture
//!
//! - **Stateless Backend**: Only stores `Arc<KeyStore>` for token management
//! - **Ephemeral Connections**: Portal proxies created per-operation (no
//!   persistent state)
//! - **Thread-Safe**: All operations are async-safe and thread-safe
//!
//! # Wayland Security Model
//!
//! Wayland's security model does not allow window enumeration. Applications
//! must:
//! 1. Use `prime_wayland_consent` tool to obtain initial permission + restore
//!    token
//! 2. Use restore tokens for subsequent headless captures
//! 3. Fall back to display capture if token expires or is revoked
//!
//! # Examples
//!
//! ```rust,ignore
//! use screenshot_mcp::{
//!     capture::{CaptureFacade, wayland_backend::WaylandBackend},
//!     model::CaptureOptions,
//!     util::key_store::KeyStore,
//! };
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() {
//!     let key_store = Arc::new(KeyStore::new());
//!     let backend = WaylandBackend::new(key_store);
//!
//!     // Capture using saved restore token
//!     let opts = CaptureOptions::default();
//!     let image = backend.capture_window("window-123".to_string(), &opts).await.unwrap();
//! }
//! ```

use std::{sync::Arc, time::Duration};

use ashpd::desktop::{
    screencast::{CursorMode, SourceType as PortalSourceType},
    PersistMode as PortalPersistMode,
};
use async_trait::async_trait;

use super::{CaptureFacade, ImageBuffer};
use crate::{
    error::{CaptureError, CaptureResult},
    model::{
        BackendType, Capabilities, CaptureOptions, PersistMode, SourceType, WindowHandle,
        WindowInfo, WindowSelector,
    },
    util::key_store::KeyStore,
};

/// Wayland screenshot backend using XDG Desktop Portal
///
/// Implements the [`CaptureFacade`] trait for Wayland compositors. Uses
/// ephemeral portal connections and token-based permissions for secure,
/// headless screenshot capture.
///
/// # Thread Safety
///
/// `WaylandBackend` is fully thread-safe (`Send + Sync`) and can be shared
/// across tasks using `Arc`. Internal portal connections are created
/// per-operation to avoid storing non-`Sync` types.
///
/// # Timeout Behavior
///
/// All portal operations have a 30-second timeout to prevent hanging on
/// stuck permission dialogs or unresponsive compositors.
#[derive(Debug)]
pub struct WaylandBackend {
    /// Token storage for restore tokens (thread-safe, shared)
    #[allow(dead_code)] // Will be used in Phase 4-6
    key_store: Arc<KeyStore>,
}

impl WaylandBackend {
    /// Creates a new WaylandBackend instance
    ///
    /// # Arguments
    ///
    /// * `key_store` - Shared token storage for restore tokens
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use screenshot_mcp::{capture::wayland_backend::WaylandBackend, util::key_store::KeyStore};
    ///
    /// let key_store = Arc::new(KeyStore::new());
    /// let backend = WaylandBackend::new(key_store);
    /// ```
    pub fn new(key_store: Arc<KeyStore>) -> Self {
        Self { key_store }
    }

    /// Creates an ephemeral portal connection
    ///
    /// Portal connections are cheap to create and don't implement `Sync`,
    /// so we create them on-demand per-operation rather than storing them.
    ///
    /// # Returns
    ///
    /// - `Ok(Screencast)` on successful connection
    /// - `Err(PortalUnavailable)` if portal service is not running
    ///
    /// # Errors
    ///
    /// Returns [`CaptureError::PortalUnavailable`] if:
    /// - xdg-desktop-portal service is not running
    /// - ScreenCast portal interface is not available
    /// - DBus connection fails
    #[allow(dead_code)] // Will be used in Phase 4-6
    async fn portal(&self) -> CaptureResult<ashpd::desktop::screencast::Screencast<'static>> {
        ashpd::desktop::screencast::Screencast::new()
            .await
            .map_err(|e| {
                tracing::error!("Failed to connect to ScreenCast portal: {}", e);
                CaptureError::PortalUnavailable {
                    portal: "org.freedesktop.portal.ScreenCast".to_string(),
                }
            })
    }

    /// Wraps a future with a timeout
    ///
    /// All portal operations should be wrapped with this to prevent hanging
    /// on stuck permission dialogs or unresponsive compositors.
    ///
    /// # Arguments
    ///
    /// * `future` - The async operation to wrap
    /// * `timeout_secs` - Timeout duration in seconds (default: 30)
    ///
    /// # Returns
    ///
    /// - `Ok(T)` if operation completes within timeout
    /// - `Err(CaptureTimeout)` if operation exceeds timeout
    #[allow(dead_code)] // Will be used in Phase 4-6
    async fn with_timeout<F, T>(future: F, timeout_secs: u64) -> CaptureResult<T>
    where
        F: std::future::Future<Output = CaptureResult<T>>,
    {
        tokio::time::timeout(Duration::from_secs(timeout_secs), future)
            .await
            .map_err(|_| {
                tracing::error!("Portal operation timed out after {}s", timeout_secs);
                CaptureError::CaptureTimeout {
                    duration_ms: timeout_secs * 1000,
                }
            })?
    }

    /// Converts our SourceType to ashpd's PortalSourceType
    fn source_type_to_portal(source_type: SourceType) -> PortalSourceType {
        match source_type {
            SourceType::Monitor => PortalSourceType::Monitor,
            SourceType::Window => PortalSourceType::Window,
            SourceType::Virtual => PortalSourceType::Virtual,
        }
    }

    /// Converts our PersistMode to ashpd's PortalPersistMode
    fn persist_mode_to_portal(persist_mode: PersistMode) -> PortalPersistMode {
        match persist_mode {
            PersistMode::DoNotPersist => PortalPersistMode::DoNot,
            PersistMode::TransientWhileRunning => PortalPersistMode::Application,
            PersistMode::PersistUntilRevoked => PortalPersistMode::ExplicitlyRevoked,
        }
    }

    /// Opens the XDG Desktop Portal screencast picker and stores restore tokens
    ///
    /// This is the core "priming" operation that requests user permission for
    /// screen capture and obtains restore tokens for future headless captures.
    ///
    /// # Workflow
    ///
    /// 1. Create portal connection
    /// 2. Create screencast session
    /// 3. Call select_sources with specified parameters
    /// 4. Call start() which shows the portal picker
    /// 5. User selects screen/window (blocks until user responds)
    /// 6. Extract streams and restore tokens from response
    /// 7. Store tokens in KeyStore with indexed IDs
    /// 8. Return result with source IDs
    ///
    /// # Arguments
    ///
    /// * `source_type` - Type of source to capture (Monitor, Window, Virtual)
    /// * `source_id` - Base identifier for stored tokens
    /// * `include_cursor` - Whether to include cursor in future captures
    ///
    /// # Returns
    ///
    /// - `Ok(PrimeConsentResult)` with source IDs and stream count
    /// - `Err(PortalUnavailable)` if portal service not running
    /// - `Err(PermissionDenied)` if user cancels or denies permission
    /// - `Err(CaptureTimeout)` if operation exceeds 30 seconds
    ///
    /// # Errors
    ///
    /// - [`CaptureError::PortalUnavailable`] - xdg-desktop-portal not
    ///   installed/running
    /// - [`CaptureError::PermissionDenied`] - User cancelled picker dialog
    /// - [`CaptureError::CaptureTimeout`] - User didn't respond within 30s
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let backend = WaylandBackend::new(key_store);
    /// let result = backend.prime_consent(
    ///     SourceType::Monitor,
    ///     "wayland-default",
    ///     false
    /// ).await?;
    ///
    /// println!("Stored token as: {}", result.primary_source_id);
    /// ```
    pub async fn prime_consent(
        &self,
        source_type: SourceType,
        source_id: &str,
        include_cursor: bool,
    ) -> CaptureResult<PrimeConsentResult> {
        // Wrap entire operation in timeout (30 seconds)
        Self::with_timeout(
            async {
                // Step 1: Create portal connection
                let proxy = self.portal().await?;

                // Step 2: Create session
                let session = proxy.create_session().await.map_err(|e| {
                    tracing::error!("Failed to create portal session: {}", e);
                    CaptureError::PortalUnavailable {
                        portal: "org.freedesktop.portal.ScreenCast".to_string(),
                    }
                })?;

                // Step 3: Select sources
                let portal_source_type = Self::source_type_to_portal(source_type);
                let persist_mode = Self::persist_mode_to_portal(PersistMode::PersistUntilRevoked);
                let cursor_mode = if include_cursor {
                    CursorMode::Embedded
                } else {
                    CursorMode::Hidden
                };

                proxy
                    .select_sources(
                        &session,
                        cursor_mode,
                        portal_source_type.into(), // Convert to BitFlags
                        false,                     /* multiple: we only support single source
                                                    * selection for now */
                        None, // restore_token: None for new session
                        persist_mode,
                    )
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to select portal sources: {}", e);
                        CaptureError::PermissionDenied {
                            platform: "Linux".to_string(),
                            backend:  BackendType::Wayland,
                        }
                    })?;

                // Step 4: Start session (shows picker to user)
                let request = proxy
                    .start(&session, None) // No parent window
                    .await
                    .map_err(|e| {
                        tracing::error!("Portal start failed: {}", e);
                        // Check if user cancelled
                        let err_str = e.to_string().to_lowercase();
                        if err_str.contains("cancel") || err_str.contains("denied") {
                            CaptureError::PermissionDenied {
                                platform: "Linux".to_string(),
                                backend:  BackendType::Wayland,
                            }
                        } else {
                            CaptureError::PortalUnavailable {
                                portal: "org.freedesktop.portal.ScreenCast".to_string(),
                            }
                        }
                    })?;

                // Get the response
                let response = request.response().map_err(|e| {
                    tracing::error!("Failed to get portal response: {}", e);
                    CaptureError::PermissionDenied {
                        platform: "Linux".to_string(),
                        backend:  BackendType::Wayland,
                    }
                })?;

                // Step 5: Extract streams and restore token
                let streams = response.streams();
                if streams.is_empty() {
                    return Err(CaptureError::PermissionDenied {
                        platform: "Linux".to_string(),
                        backend:  BackendType::Wayland,
                    });
                }

                // Step 6: Get restore token from response (single token for entire session)
                let token =
                    response
                        .restore_token()
                        .ok_or_else(|| CaptureError::PermissionDenied {
                            platform: "Linux".to_string(),
                            backend:  BackendType::Wayland,
                        })?;

                // Store single token for the source_id
                self.key_store.store_token(source_id, token)?;

                tracing::info!(
                    "Stored restore token for source '{}' ({} stream(s))",
                    source_id,
                    streams.len()
                );

                // Step 7: Return result
                Ok(PrimeConsentResult {
                    primary_source_id: source_id.to_string(),
                    all_source_ids:    vec![source_id.to_string()],
                    num_streams:       streams.len(),
                })
            },
            30, // 30-second timeout
        )
        .await
    }
}

/// Result of prime_consent operation
///
/// Contains the source IDs where restore tokens were stored and metadata
/// about the consent session.
#[derive(Debug, Clone)]
pub struct PrimeConsentResult {
    /// Primary source ID (for single stream or first of multiple)
    pub primary_source_id: String,
    /// All source IDs (includes primary)
    pub all_source_ids:    Vec<String>,
    /// Number of streams/sources captured
    pub num_streams:       usize,
}

#[async_trait]
impl CaptureFacade for WaylandBackend {
    /// Lists all capturable windows (NOT SUPPORTED on Wayland)
    ///
    /// Wayland's security model does not allow window enumeration. Applications
    /// must use the `prime_wayland_consent` tool to obtain permission and a
    /// restore token for headless capture.
    ///
    /// # Returns
    ///
    /// Always returns [`CaptureError::BackendNotAvailable`] with a remediation
    /// hint explaining how to use Wayland restore tokens.
    ///
    /// # Errors
    ///
    /// This method always fails with actionable guidance for users.
    async fn list_windows(&self) -> CaptureResult<Vec<WindowInfo>> {
        Err(CaptureError::BackendNotAvailable {
            backend: BackendType::Wayland,
        })
    }

    /// Resolves a window selector to a window handle
    ///
    /// This method validates Wayland source IDs and checks for stored restore
    /// tokens. On Wayland, window handles use the format
    /// `wayland:<source-id>` where `source-id` corresponds to a stored
    /// restore token from a previous `prime_consent()` call.
    ///
    /// # Arguments
    ///
    /// * `selector` - Window selector with `exe` field containing
    ///   "wayland:<source-id>"
    ///
    /// # Returns
    ///
    /// - `Ok(source_id)` if token exists in KeyStore
    /// - `Err(TokenNotFound)` if no token exists for source_id
    /// - `Err(InvalidParameter)` if selector format is invalid
    /// - `Err(WindowNotFound)` if selector doesn't match Wayland pattern
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // After calling prime_consent with source_id="wayland-default"
    /// let selector = WindowSelector { exe: Some("wayland:wayland-default".to_string()), .. };
    /// let handle = backend.resolve_target(&selector).await?;
    /// // handle == "wayland-default"
    /// ```
    ///
    /// # Integration with capture_window Tool
    ///
    /// The MCP `capture_window` tool uses this method when the selector
    /// contains an `exe` field with the "wayland:" prefix, enabling
    /// seamless integration with primed Wayland sources.
    async fn resolve_target(&self, selector: &WindowSelector) -> CaptureResult<WindowHandle> {
        // Check if selector has any criteria
        if selector.title_substring_or_regex.is_none()
            && selector.class.is_none()
            && selector.exe.is_none()
        {
            return Err(CaptureError::InvalidParameter {
                parameter: "selector".to_string(),
                reason:    "At least one selector field must be specified".to_string(),
            });
        }

        // Check for "wayland:" prefix in exe field
        if let Some(exe) = &selector.exe {
            if let Some(source_id) = exe.strip_prefix("wayland:") {
                // Validate that source_id is not empty
                if source_id.is_empty() {
                    return Err(CaptureError::InvalidParameter {
                        parameter: "exe".to_string(),
                        reason:    "Wayland source ID cannot be empty (format: \
                                    'wayland:<source-id>')"
                            .to_string(),
                    });
                }

                // Check if token exists in KeyStore
                if self.key_store.has_token(source_id)? {
                    tracing::info!("Resolved Wayland source ID: {}", source_id);
                    return Ok(source_id.to_string());
                } else {
                    return Err(CaptureError::TokenNotFound {
                        source_id: source_id.to_string(),
                    });
                }
            }
        }

        // If we get here, selector doesn't match Wayland pattern
        Err(CaptureError::WindowNotFound {
            selector: selector.clone(),
        })
    }

    /// Captures a screenshot of a specific window using restore token
    ///
    /// This method implements the core Wayland capture flow:
    /// 1. Retrieve restore token from KeyStore
    /// 2. Call portal with token to restore session
    /// 3. Capture screenshot frame
    /// 4. Rotate token (portal returns new single-use token)
    /// 5. Apply transformations (scale, crop) from CaptureOptions
    ///
    /// If restore fails, falls back to display capture + region crop.
    ///
    /// # Arguments
    ///
    /// * `handle` - Window handle (source ID with token in KeyStore)
    /// * `opts` - Capture options (format, quality, scale, region, cursor)
    ///
    /// # Returns
    ///
    /// - `Ok(ImageBuffer)` with captured screenshot
    /// - `Err(TokenNotFound)` if no token exists for this handle
    /// - `Err(CaptureTimeout)` if operation exceeds 30 seconds
    /// - `Err(PortalUnavailable)` if portal service not running
    ///
    /// # Errors
    ///
    /// - [`CaptureError::TokenNotFound`] - No restore token for this source
    /// - [`CaptureError::CaptureTimeout`] - Operation took >30 seconds
    /// - [`CaptureError::PortalUnavailable`] - Portal service unavailable
    /// - [`CaptureError::PermissionDenied`] - User denied permission
    async fn capture_window(
        &self,
        _handle: WindowHandle,
        _opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        // Phase 3 stub: Full implementation in Phase 5
        // For now, return an error indicating the feature is not yet implemented
        Err(CaptureError::BackendNotAvailable {
            backend: BackendType::Wayland,
        })
    }

    /// Captures a screenshot of an entire display
    ///
    /// Opens the XDG Desktop Portal picker for the user to select which
    /// display to capture. This is the fallback method when restore tokens
    /// are not available or have expired.
    ///
    /// # Arguments
    ///
    /// * `display_id` - Display identifier (ignored on Wayland - user selects
    ///   via portal)
    /// * `opts` - Capture options (format, quality, scale, region, cursor)
    ///
    /// # Returns
    ///
    /// - `Ok(ImageBuffer)` with captured screenshot
    /// - `Err(CaptureTimeout)` if operation exceeds 30 seconds
    /// - `Err(PermissionDenied)` if user cancels portal dialog
    ///
    /// # Errors
    ///
    /// - [`CaptureError::CaptureTimeout`] - Operation took >30 seconds
    /// - [`CaptureError::PortalUnavailable`] - Portal service unavailable
    /// - [`CaptureError::PermissionDenied`] - User denied permission
    async fn capture_display(
        &self,
        _display_id: Option<u32>,
        _opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        // Phase 3 stub: Full implementation in Phase 6
        // For now, return an error indicating the feature is not yet implemented
        Err(CaptureError::BackendNotAvailable {
            backend: BackendType::Wayland,
        })
    }

    /// Returns the capabilities of this Wayland backend
    ///
    /// Wayland backends support:
    /// - Display capture: Yes (via portal picker)
    /// - Window capture: No (security limitation)
    /// - Cursor inclusion: Yes (via portal option)
    /// - Region cropping: Yes (post-capture)
    /// - Restore tokens: Yes (permission-free recapture)
    ///
    /// # Returns
    ///
    /// A [`Capabilities`] struct describing Wayland-specific features.
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_cursor:          true,  // Portal supports cursor option
            supports_region:          true,  // Post-capture cropping
            supports_wayland_restore: true,  // Restore tokens for headless capture
            supports_window_capture:  false, // Wayland security limitation
            supports_display_capture: true,  // Via portal picker
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wayland_backend_new() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        // Should construct without error
        assert!(format!("{:?}", backend).contains("WaylandBackend"));
    }

    #[test]
    fn test_capabilities() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);
        let caps = backend.capabilities();

        assert!(caps.supports_cursor);
        assert!(caps.supports_region);
        assert!(caps.supports_wayland_restore);
        assert!(!caps.supports_window_capture); // Wayland limitation
        assert!(caps.supports_display_capture);
    }

    #[tokio::test]
    async fn test_list_windows_returns_error() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        let result = backend.list_windows().await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CaptureError::BackendNotAvailable { .. }));
    }

    #[tokio::test]
    async fn test_resolve_target_validates_selector() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        // Empty selector should fail
        let selector = WindowSelector {
            title_substring_or_regex: None,
            class: None,
            exe: None,
        };
        let result = backend.resolve_target(&selector).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CaptureError::InvalidParameter { .. }));
    }

    #[tokio::test]
    async fn test_resolve_target_wayland_prefix_empty_source_id() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        // Wayland prefix with empty source_id should fail
        let selector = WindowSelector {
            title_substring_or_regex: None,
            class: None,
            exe: Some("wayland:".to_string()),
        };
        let result = backend.resolve_target(&selector).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CaptureError::InvalidParameter { .. }));
    }

    #[tokio::test]
    async fn test_resolve_target_wayland_prefix_no_token() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        // Wayland prefix with valid source_id but no token should fail
        let selector = WindowSelector {
            title_substring_or_regex: None,
            class: None,
            exe: Some("wayland:test-source".to_string()),
        };
        let result = backend.resolve_target(&selector).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CaptureError::TokenNotFound { .. }));
    }

    #[tokio::test]
    async fn test_resolve_target_wayland_prefix_with_token() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(Arc::clone(&key_store));

        // Store a token
        key_store
            .store_token("test-source", "test-token")
            .expect("Failed to store token");

        // Wayland prefix with valid source_id and token should succeed
        let selector = WindowSelector {
            title_substring_or_regex: None,
            class: None,
            exe: Some("wayland:test-source".to_string()),
        };
        let result = backend.resolve_target(&selector).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-source");
    }

    #[tokio::test]
    async fn test_resolve_target_non_wayland_selector() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        // Non-wayland selector should fail with WindowNotFound
        let selector = WindowSelector::by_title("Firefox");
        let result = backend.resolve_target(&selector).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CaptureError::WindowNotFound { .. }));
    }

    #[tokio::test]
    async fn test_capture_window_stub() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        let opts = CaptureOptions::default();
        let result = backend
            .capture_window("window-123".to_string(), &opts)
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CaptureError::BackendNotAvailable { .. }));
    }

    #[tokio::test]
    async fn test_capture_display_stub() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        let opts = CaptureOptions::default();
        let result = backend.capture_display(None, &opts).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CaptureError::BackendNotAvailable { .. }));
    }
}
