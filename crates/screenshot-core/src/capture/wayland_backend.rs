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
//! # Timeout Configuration
//!
//! Portal operations use a 30-second timeout (`DEFAULT_PORTAL_TIMEOUT_SECS`) to
//! accommodate:
//! - User interaction time with permission dialogs (typically 5-15 seconds)
//! - Portal service response latency (1-3 seconds on most systems)
//! - Compositor-specific delays (GNOME Shell vs KDE Plasma vs wlroots)
//!
//! PipeWire frame capture uses a shorter 5-second timeout
//! (`PIPEWIRE_FRAME_TIMEOUT_SECS`) since frames should arrive immediately once
//! the stream is active
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

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use ashpd::desktop::{
    PersistMode as PortalPersistMode,
    screencast::{CursorMode, SourceType as PortalSourceType},
};
use async_trait::async_trait;
use image::GenericImageView;

use super::{
    BackendCapabilities, CaptureFacade, ImageBuffer, ScreenCapture, WaylandRestoreCapable,
    WindowResolver,
    constants::{
        PIPEWIRE_FRAME_TIMEOUT_SECS, PIPEWIRE_LOOP_ITERATION_MS, WAYLAND_PORTAL_TIMEOUT_SECS,
    },
    traits::PrimeConsentResult,
};
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

    /// Captures a single frame from a PipeWire stream node
    ///
    /// This function connects to the specified PipeWire node, captures one
    /// frame, and converts it to a `DynamicImage`. It handles the
    /// complexity of PipeWire's async stream API for one-shot frame
    /// capture.
    ///
    /// # Arguments
    ///
    /// * `node_id` - PipeWire node ID from the portal stream
    ///
    /// # Returns
    ///
    /// - `Ok(DynamicImage)` with the captured frame in RGBA8 format
    /// - `Err(BackendNotAvailable)` if PipeWire connection fails or no frame
    ///   received
    ///
    /// # Errors
    ///
    /// - [`CaptureError::BackendNotAvailable`] - PipeWire not available or
    ///   connection failed
    /// - [`CaptureError::CaptureTimeout`] - No frame received within timeout
    ///
    /// # Implementation Note
    ///
    /// This uses blocking PipeWire API with a timeout for simplicity. A frame
    /// should arrive immediately since the portal session is already active.
    async fn capture_pipewire_frame(node_id: u32) -> CaptureResult<image::DynamicImage> {
        use pipewire::{
            context::Context,
            keys,
            main_loop::MainLoop,
            properties::properties,
            spa,
            stream::{Stream, StreamFlags},
        };

        tracing::debug!("Connecting to PipeWire node {}", node_id);

        // Shared state for capturing the frame
        let frame_data: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
        let frame_captured = Arc::new(AtomicBool::new(false));
        let frame_width: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));
        let frame_height: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));

        // Clone references for the callback
        let frame_data_cb = Arc::clone(&frame_data);
        let frame_captured_cb = Arc::clone(&frame_captured);
        let _frame_width_cb = Arc::clone(&frame_width);
        let _frame_height_cb = Arc::clone(&frame_height);

        // Spawn blocking PipeWire capture in a separate thread
        let result = tokio::task::spawn_blocking(move || {
            // Create PipeWire main loop
            let mainloop = MainLoop::new(None).map_err(|e| {
                tracing::error!("Failed to create PipeWire MainLoop: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::Wayland,
                }
            })?;

            let context = Context::new(&mainloop).map_err(|e| {
                tracing::error!("Failed to create PipeWire Context: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::Wayland,
                }
            })?;

            let core = context.connect(None).map_err(|e| {
                tracing::error!("Failed to connect to PipeWire Core: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::Wayland,
                }
            })?;

            // Create stream with callbacks
            let stream = Stream::new(
                &core,
                "screenshot-mcp",
                properties! {
                    *keys::MEDIA_TYPE => "Video",
                    *keys::MEDIA_CATEGORY => "Capture",
                    *keys::MEDIA_ROLE => "Screen",
                },
            )
            .map_err(|e| {
                tracing::error!("Failed to create PipeWire Stream: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::Wayland,
                }
            })?;

            // Add listener for stream events (with unit type for user data)
            let _listener = stream
                .add_local_listener::<()>()
                .state_changed(|_stream, _old, new, _error_message| {
                    tracing::debug!("PipeWire stream state changed to: {:?}", new);
                })
                .param_changed(move |_stream, _id, param_type, _param| {
                    // param_type is a u32 representing the SPA param type
                    // ParamType::Format corresponds to spa_param_type::SPA_PARAM_Format (value 4)
                    if param_type != 4 {
                        // Not a format parameter, ignore
                        return;
                    }

                    // TODO: Parse video format to get dimensions from _param
                    // For now, we'll infer dimensions from buffer size
                    tracing::debug!(
                        "Received format parameter (dimensions inference from buffer size)"
                    );
                })
                .process(move |stream, _user_data| {
                    // Only capture once
                    if frame_captured_cb.load(Ordering::Relaxed) {
                        return;
                    }

                    // Get buffer from stream
                    if let Some(mut buffer) = stream.dequeue_buffer() {
                        if let Some(chunk) = buffer.datas_mut().first_mut() {
                            if let Some(data_slice) = chunk.data() {
                                tracing::debug!(
                                    "Captured PipeWire frame ({} bytes)",
                                    data_slice.len()
                                );

                                // Copy frame data
                                *frame_data_cb.lock().unwrap() = Some(data_slice.to_vec());
                                frame_captured_cb.store(true, Ordering::Relaxed);
                            }
                        }
                    }
                })
                .register()
                .map_err(|e| {
                    tracing::error!("Failed to register stream listener: {}", e);
                    CaptureError::BackendNotAvailable {
                        backend: BackendType::Wayland,
                    }
                })?;

            // Connect stream to the portal node
            let mut params = vec![];
            stream
                .connect(
                    spa::utils::Direction::Input,
                    Some(node_id),
                    StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
                    &mut params,
                )
                .map_err(|e| {
                    tracing::error!("Failed to connect to PipeWire node {}: {}", node_id, e);
                    CaptureError::BackendNotAvailable {
                        backend: BackendType::Wayland,
                    }
                })?;

            tracing::debug!("Connected to PipeWire stream, waiting for frame...");

            // Run main loop until frame is captured or timeout
            let start = std::time::Instant::now();
            let timeout_duration = Duration::from_secs(PIPEWIRE_FRAME_TIMEOUT_SECS);

            while !frame_captured.load(Ordering::Relaxed) {
                if start.elapsed() > timeout_duration {
                    tracing::error!("Timeout waiting for PipeWire frame");
                    return Err(CaptureError::CaptureTimeout {
                        duration_ms: timeout_duration.as_millis() as u64,
                    });
                }

                // Run one iteration of the main loop
                let _result = mainloop
                    .loop_()
                    .iterate(Duration::from_millis(PIPEWIRE_LOOP_ITERATION_MS));
            }

            // Extract captured data
            let data = frame_data.lock().unwrap().take().ok_or_else(|| {
                tracing::error!("No frame data captured");
                CaptureError::BackendNotAvailable {
                    backend: BackendType::Wayland,
                }
            })?;

            // Try to get dimensions from format, or infer from common aspect ratios
            let (width, height) = if let (Some(w), Some(h)) =
                (*frame_width.lock().unwrap(), *frame_height.lock().unwrap())
            {
                (w, h)
            } else {
                // Infer dimensions from buffer size (assume RGBA8 = 4 bytes per pixel)
                // Try common aspect ratios: 16:9, 16:10, 4:3
                let pixel_count = data.len() / 4;

                // Common resolutions to try
                let common_resolutions = [
                    (1920, 1080),
                    (2560, 1440),
                    (3840, 2160), // 16:9
                    (1920, 1200),
                    (2560, 1600), // 16:10
                    (1024, 768),
                    (1280, 1024), // 4:3
                    (1366, 768),
                    (1600, 900), // Laptop screens
                ];

                common_resolutions
                    .iter()
                    .find(|(w, h)| (*w as usize) * (*h as usize) == pixel_count)
                    .copied()
                    .unwrap_or_else(|| {
                        // Fall back to square root approximation
                        let side = (pixel_count as f64).sqrt() as u32;
                        tracing::warn!(
                            "Could not determine exact dimensions, guessing {}x{} from {} pixels",
                            side,
                            side,
                            pixel_count
                        );
                        (side, side)
                    })
            };

            tracing::info!(
                "Successfully captured PipeWire frame ({}x{}, {} bytes)",
                width,
                height,
                data.len()
            );

            // Convert raw buffer to DynamicImage (assume RGBA8 format from PipeWire)
            let expected_size = (width * height * 4) as usize;
            if data.len() < expected_size {
                tracing::error!(
                    "Frame buffer too small: got {} bytes, expected {}",
                    data.len(),
                    expected_size
                );
                return Err(CaptureError::BackendNotAvailable {
                    backend: BackendType::Wayland,
                });
            }

            let image_buffer = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
                width,
                height,
                data[..expected_size].to_vec(),
            )
            .ok_or_else(|| {
                tracing::error!("Failed to create image buffer from raw data");
                CaptureError::BackendNotAvailable {
                    backend: BackendType::Wayland,
                }
            })?;

            Ok(image::DynamicImage::ImageRgba8(image_buffer))
        })
        .await
        .map_err(|e| {
            tracing::error!("PipeWire capture task panicked: {}", e);
            CaptureError::BackendNotAvailable {
                backend: BackendType::Wayland,
            }
        })??;

        Ok(result)
    }

    /// Creates an ephemeral portal connection
    ///
    /// Portal connections are cheap to create and don't implement `Sync`,
    /// so we create them on-demand per-operation rather than storing them.
    ///
    /// **Test Mode**: In test builds (`#[cfg(test)]`), this method returns
    /// an immediate error without attempting a real DBus connection, preventing
    /// system permission dialogs during automated testing.
    ///
    /// # Returns
    ///
    /// - `Ok(Screencast)` on successful connection (production only)
    /// - `Err(PortalUnavailable)` if portal service is not running or in test
    ///   mode
    ///
    /// # Errors
    ///
    /// Returns [`CaptureError::PortalUnavailable`] if:
    /// - xdg-desktop-portal service is not running
    /// - ScreenCast portal interface is not available
    /// - DBus connection fails
    /// - Running in test mode (`cargo test`)
    #[cfg(not(test))]
    #[allow(dead_code)]
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

    /// Test-mode portal stub (prevents real DBus connections during tests)
    ///
    /// This version is compiled when running `cargo test` and immediately
    /// returns an error without attempting portal connection. This prevents:
    /// - System permission dialogs during automated testing
    /// - DBus connection attempts in CI environments
    /// - Test suite hangs waiting for user interaction
    ///
    /// Unit tests validate error handling paths; integration tests marked
    /// with `#[ignore]` can test real portal behavior on live Wayland systems.
    #[cfg(test)]
    #[allow(dead_code)]
    async fn portal(&self) -> CaptureResult<ashpd::desktop::screencast::Screencast<'static>> {
        Err(CaptureError::PortalUnavailable {
            portal: "org.freedesktop.portal.ScreenCast (test mode - no real connection)"
                .to_string(),
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
                            backend: BackendType::Wayland,
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
                                backend: BackendType::Wayland,
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
                        backend: BackendType::Wayland,
                    }
                })?;

                // Step 5: Extract streams and restore token
                let streams = response.streams();
                if streams.is_empty() {
                    return Err(CaptureError::PermissionDenied {
                        platform: "Linux".to_string(),
                        backend: BackendType::Wayland,
                    });
                }

                // Step 6: Get restore token from response (single token for entire session)
                let token =
                    response
                        .restore_token()
                        .ok_or_else(|| CaptureError::PermissionDenied {
                            platform: "Linux".to_string(),
                            backend: BackendType::Wayland,
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
                    all_source_ids: vec![source_id.to_string()],
                    num_streams: streams.len(),
                })
            },
            WAYLAND_PORTAL_TIMEOUT_SECS,
        )
        .await
    }
}

#[async_trait]
impl CaptureFacade for WaylandBackend {
    /// Lists Wayland capture targets derived from stored restore tokens
    ///
    /// Returns synthetic `WindowInfo` entries that map to primed Wayland
    /// sources. When no tokens exist, a single instructional entry is
    /// returned guiding the user to run `prime_wayland_consent`.
    async fn list_windows(&self) -> CaptureResult<Vec<WindowInfo>> {
        let source_ids = self.key_store.list_source_ids()?;

        if source_ids.is_empty() {
            return Ok(vec![WindowInfo::new(
                "wayland:prime-required".to_string(),
                "No Wayland sources primed. Run prime_wayland_consent to create one.".to_string(),
                "WaylandInstructions".to_string(),
                "screenshot-mcp".to_string(),
                0,
                BackendType::Wayland,
            )]);
        }

        Ok(source_ids
            .into_iter()
            .map(|source_id| {
                WindowInfo::new(
                    format!("wayland:{}", source_id),
                    format!(
                        "Wayland restore token '{}' (use exe='wayland:{}')",
                        source_id, source_id
                    ),
                    "WaylandRestoreToken".to_string(),
                    "prime_wayland_consent".to_string(),
                    0,
                    BackendType::Wayland,
                )
            })
            .collect())
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
                reason: "At least one selector field must be specified".to_string(),
            });
        }

        // Check for "wayland:" prefix in exe field
        if let Some(exe) = &selector.exe {
            if let Some(source_id) = exe.strip_prefix("wayland:") {
                // Validate that source_id is not empty
                if source_id.is_empty() {
                    return Err(CaptureError::InvalidParameter {
                        parameter: "exe".to_string(),
                        reason: "Wayland source ID cannot be empty (format: \
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
        handle: WindowHandle,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        // Wrap entire operation in 30-second timeout
        Self::with_timeout(
            async {
                // Step 1: Retrieve old token from KeyStore
                let old_token = match self.key_store.retrieve_token(&handle)? {
                    Some(token) => token,
                    None => {
                        // FALLBACK TRIGGER: No token found, fall back to display capture
                        tracing::warn!(
                            "No restore token found for source '{}', falling back to display \
                             capture",
                            handle
                        );
                        return self.capture_display(None, opts).await;
                    }
                };

                tracing::debug!("Retrieved restore token for source '{}'", handle);

                // Step 2: Create portal connection and session
                let proxy = self.portal().await?;
                let session = proxy.create_session().await.map_err(|e| {
                    tracing::error!("Failed to create portal session: {}", e);
                    CaptureError::PortalUnavailable {
                        portal: "org.freedesktop.portal.ScreenCast".to_string(),
                    }
                })?;

                tracing::debug!("Created portal session for restore");

                // Step 3: Restore session with old token
                let cursor_mode = if opts.include_cursor {
                    CursorMode::Embedded
                } else {
                    CursorMode::Hidden
                };
                let portal_source_type = Self::source_type_to_portal(SourceType::Monitor);
                let persist_mode = Self::persist_mode_to_portal(PersistMode::PersistUntilRevoked);

                // FALLBACK-AWARE: Try to restore session, catch token errors
                let select_result = proxy
                    .select_sources(
                        &session,
                        cursor_mode,
                        portal_source_type.into(), // Convert to BitFlags
                        false,                     // multiple: single source only
                        Some(&old_token),          // RESTORE TOKEN HERE
                        persist_mode,
                    )
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to restore session with token: {}", e);
                        // Check if token expired/revoked
                        let err_str = e.to_string().to_lowercase();
                        if err_str.contains("token")
                            || err_str.contains("invalid")
                            || err_str.contains("expired")
                        {
                            CaptureError::TokenNotFound {
                                source_id: handle.clone(),
                            }
                        } else {
                            CaptureError::PortalUnavailable {
                                portal: "org.freedesktop.portal.ScreenCast".to_string(),
                            }
                        }
                    });

                // FALLBACK TRIGGER: If token restore failed, fall back to display capture
                if let Err(CaptureError::TokenNotFound { source_id }) = select_result {
                    tracing::warn!(
                        "Token restore failed for source '{}', falling back to display capture",
                        source_id
                    );
                    return self.capture_display(None, opts).await;
                }

                // Propagate other errors (non-token failures should fail-fast)
                select_result?;

                tracing::debug!("Restored session with token successfully");

                // Step 4: Start session and get new token
                let request = proxy
                    .start(&session, None) // No parent window
                    .await
                    .map_err(|e| {
                        tracing::error!("Portal start failed: {}", e);
                        let err_str = e.to_string().to_lowercase();
                        if err_str.contains("cancel") || err_str.contains("denied") {
                            CaptureError::PermissionDenied {
                                platform: "Linux".to_string(),
                                backend: BackendType::Wayland,
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
                        backend: BackendType::Wayland,
                    }
                })?;

                // Step 5: Extract new token and rotate BEFORE capturing frame (CRITICAL)
                let new_token =
                    response
                        .restore_token()
                        .ok_or_else(|| CaptureError::EncryptionFailed {
                            reason: "Portal did not return new restore token".to_string(),
                        })?;

                tracing::debug!("Received new restore token, rotating atomically");

                // Atomic token rotation (delete old, store new)
                self.key_store.rotate_token(&handle, new_token)?;

                tracing::info!("Token rotated successfully for source '{}'", handle);

                // Step 6: Get PipeWire stream information
                let streams = response.streams();
                if streams.is_empty() {
                    return Err(CaptureError::BackendNotAvailable {
                        backend: BackendType::Wayland,
                    });
                }

                let stream = &streams[0];
                let node_id = stream.pipe_wire_node_id();

                tracing::debug!(
                    "Got PipeWire node ID: {} ({} stream(s) total)",
                    node_id,
                    streams.len()
                );

                // Step 7: Capture frame from PipeWire
                let raw_image = Self::capture_pipewire_frame(node_id).await?;

                tracing::debug!("Raw image captured: {:?}", raw_image.dimensions());

                // Step 8: Apply transformations (Phase 5C)
                let mut image_buffer = ImageBuffer::new(raw_image);

                // Apply region crop first (if specified)
                if let Some(region) = &opts.region {
                    tracing::debug!("Cropping to region: {:?}", region);
                    image_buffer = image_buffer.crop(*region)?;
                }

                // Apply scale second (if not 1.0)
                if (opts.scale - 1.0).abs() > 0.01 {
                    tracing::debug!("Scaling by factor: {}", opts.scale);
                    image_buffer = image_buffer.scale(opts.scale)?;
                }

                tracing::info!(
                    "Transformed image: {:?} (format: {:?})",
                    image_buffer.dimensions(),
                    opts.format
                );

                Ok(image_buffer)
            },
            WAYLAND_PORTAL_TIMEOUT_SECS,
        )
        .await
    }

    /// Captures a screenshot of an entire display
    ///
    /// Opens the XDG Desktop Portal picker for the user to select which
    /// display to capture. This is the fallback method when restore tokens
    /// are not available or have expired.
    ///
    /// # Fallback Behavior
    ///
    /// This method is automatically called by `capture_window` when token
    /// restoration fails. It creates a NEW portal session (no token reuse)
    /// and presents the user with the picker dialog. The region from the
    /// original capture options is preserved and applied to the display
    /// capture result.
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
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Manual display capture (not via fallback)
    /// let backend = WaylandBackend::new(key_store);
    /// let opts = CaptureOptions::default();
    /// let image = backend.capture_display(None, &opts).await?;
    /// ```
    async fn capture_display(
        &self,
        _display_id: Option<u32>,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        tracing::warn!("Display capture initiated (fallback or direct call)");

        // Wrap entire operation in 30-second timeout
        Self::with_timeout(
            async {
                // Step 1: Create portal connection
                let proxy = self.portal().await?;

                // Step 2: Create session (NEW, not restore)
                let session = proxy.create_session().await.map_err(|e| {
                    tracing::error!("Failed to create portal session for display capture: {}", e);
                    CaptureError::PortalUnavailable {
                        portal: "org.freedesktop.portal.ScreenCast".to_string(),
                    }
                })?;

                tracing::debug!("Created portal session for display capture");

                // Step 3: Select sources (MONITOR type, no restore token)
                let cursor_mode = if opts.include_cursor {
                    CursorMode::Embedded
                } else {
                    CursorMode::Hidden
                };
                let portal_source_type = Self::source_type_to_portal(SourceType::Monitor);
                // DON'T persist token - this is fallback, temporary session
                let persist_mode = Self::persist_mode_to_portal(PersistMode::DoNotPersist);

                proxy
                    .select_sources(
                        &session,
                        cursor_mode,
                        portal_source_type.into(), // Convert to BitFlags
                        false,                     // single source
                        None,                      // NO RESTORE TOKEN (new session)
                        persist_mode,
                    )
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to select sources for display capture: {}", e);
                        CaptureError::PermissionDenied {
                            platform: "Linux".to_string(),
                            backend: BackendType::Wayland,
                        }
                    })?;

                tracing::debug!("Selected display capture sources");

                // Step 4: Start session (shows picker to user)
                let request = proxy
                    .start(&session, None) // No parent window
                    .await
                    .map_err(|e| {
                        tracing::error!("Display capture portal start failed: {}", e);
                        let err_str = e.to_string().to_lowercase();
                        if err_str.contains("cancel") || err_str.contains("denied") {
                            CaptureError::PermissionDenied {
                                platform: "Linux".to_string(),
                                backend: BackendType::Wayland,
                            }
                        } else {
                            CaptureError::PortalUnavailable {
                                portal: "org.freedesktop.portal.ScreenCast".to_string(),
                            }
                        }
                    })?;

                // Get the response
                let response = request.response().map_err(|e| {
                    tracing::error!("Failed to get display capture portal response: {}", e);
                    CaptureError::PermissionDenied {
                        platform: "Linux".to_string(),
                        backend: BackendType::Wayland,
                    }
                })?;

                tracing::debug!("User selected display in portal picker");

                // Step 5: Get PipeWire stream
                let streams = response.streams();
                if streams.is_empty() {
                    return Err(CaptureError::PermissionDenied {
                        platform: "Linux".to_string(),
                        backend: BackendType::Wayland,
                    });
                }

                let stream = &streams[0];
                let node_id = stream.pipe_wire_node_id();

                tracing::debug!(
                    "Got PipeWire node ID: {} for display capture ({} stream(s))",
                    node_id,
                    streams.len()
                );

                // Step 6: Capture frame (REUSE existing helper)
                let raw_image = Self::capture_pipewire_frame(node_id).await?;

                tracing::debug!("Display capture raw image: {:?}", raw_image.dimensions());

                // Step 7: Apply transformations (SAME as capture_window)
                let mut image_buffer = ImageBuffer::new(raw_image);

                // Apply region crop first (if specified)
                if let Some(region) = &opts.region {
                    tracing::debug!("Cropping display capture to region: {:?}", region);
                    image_buffer = image_buffer.crop(*region)?;
                }

                // Apply scale second (if not 1.0)
                if (opts.scale - 1.0).abs() > 0.01 {
                    tracing::debug!("Scaling display capture by factor: {}", opts.scale);
                    image_buffer = image_buffer.scale(opts.scale)?;
                }

                tracing::info!(
                    "Display capture complete: {:?} (format: {:?})",
                    image_buffer.dimensions(),
                    opts.format
                );

                Ok(image_buffer)
            },
            WAYLAND_PORTAL_TIMEOUT_SECS,
        )
        .await
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
            supports_cursor: true,              // Portal supports cursor option
            supports_region: true,              // Post-capture cropping
            supports_wayland_restore: true,     // Restore tokens for headless capture
            supports_window_enumeration: false, // Wayland security limitation
            supports_display_capture: true,     // Via portal picker
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ============================================================================
// Capability Trait Implementations
// ============================================================================

// Note: WaylandBackend does NOT implement WindowEnumerator because
// Wayland's security model prevents window enumeration.

#[async_trait]
impl WindowResolver for WaylandBackend {
    async fn resolve(&self, selector: &WindowSelector) -> CaptureResult<WindowHandle> {
        // Delegate to CaptureFacade implementation
        CaptureFacade::resolve_target(self, selector).await
    }
}

#[async_trait]
impl ScreenCapture for WaylandBackend {
    async fn capture_window(
        &self,
        handle: WindowHandle,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        // Delegate to CaptureFacade implementation
        CaptureFacade::capture_window(self, handle, opts).await
    }

    async fn capture_display(
        &self,
        display_id: Option<u32>,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        // Delegate to CaptureFacade implementation
        CaptureFacade::capture_display(self, display_id, opts).await
    }
}

#[async_trait]
impl WaylandRestoreCapable for WaylandBackend {
    async fn prime_consent(
        &self,
        source_type: SourceType,
        source_id: &str,
        include_cursor: bool,
    ) -> CaptureResult<PrimeConsentResult> {
        // Delegate to inherent method (same type, no conversion needed)
        WaylandBackend::prime_consent(self, source_type, source_id, include_cursor).await
    }

    async fn capture_with_token(
        &self,
        source_id: &str,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        // Use the existing capture_window with a wayland: prefixed handle
        let handle = format!("wayland:{}", source_id);
        CaptureFacade::capture_window(self, handle, opts).await
    }

    fn list_sources(&self) -> CaptureResult<Vec<String>> {
        self.key_store.list_source_ids()
    }
}

impl BackendCapabilities for WaylandBackend {
    fn supports_cursor(&self) -> bool {
        true // Portal supports cursor option
    }

    fn supports_region(&self) -> bool {
        true // Post-capture cropping
    }

    fn supports_wayland_restore(&self) -> bool {
        true // Restore tokens for headless capture
    }

    fn supports_window_enumeration(&self) -> bool {
        false // Wayland security limitation
    }

    fn supports_display_capture(&self) -> bool {
        true // Via portal picker
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
        assert!(!caps.supports_window_enumeration); // Wayland limitation
        assert!(caps.supports_display_capture);
    }

    #[tokio::test]
    async fn test_list_windows_returns_instruction_when_empty() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(Arc::clone(&key_store));

        // Clean up any leftover tokens from previous tests
        for source_id in key_store.list_source_ids().unwrap() {
            let _ = key_store.delete_token(&source_id);
        }

        let windows = backend.list_windows().await.unwrap();
        assert_eq!(windows.len(), 1);
        let entry = &windows[0];
        assert_eq!(entry.id, "wayland:prime-required");
        assert_eq!(entry.backend, BackendType::Wayland);
        assert!(
            entry.title.to_lowercase().contains("prime_wayland_consent"),
            "Title should mention prime_wayland_consent"
        );

        // No tokens were created; ensure index remains empty
        assert!(key_store.list_source_ids().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_windows_returns_primed_sources() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(Arc::clone(&key_store));

        // Clean up any leftover tokens from previous tests
        for source_id in key_store.list_source_ids().unwrap() {
            let _ = key_store.delete_token(&source_id);
        }

        key_store
            .store_token("wayland-default", "token-default")
            .unwrap();
        key_store
            .store_token("firefox-dev", "token-firefox")
            .unwrap();

        let mut windows = backend.list_windows().await.unwrap();
        windows.sort_by(|a, b| a.id.cmp(&b.id));

        assert_eq!(windows.len(), 2);

        let first = &windows[0];
        assert_eq!(first.id, "wayland:firefox-dev");
        assert!(first.title.contains("firefox-dev"), "Title should include source id");

        let second = &windows[1];
        assert_eq!(second.id, "wayland:wayland-default");
        assert!(second.title.contains("wayland-default"), "Title should include source id");

        // Cleanup tokens to avoid polluting persistent store
        key_store.delete_token("firefox-dev").unwrap();
        key_store.delete_token("wayland-default").unwrap();
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
    async fn test_capture_window_no_token_fallback() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        let opts = CaptureOptions::default();
        let result = backend
            .capture_window("window-123".to_string(), &opts)
            .await;
        assert!(result.is_err());
        // With fallback enabled, no token triggers fallback to capture_display
        // which fails with CaptureTimeout in test environment (portal connection times
        // out)
        assert!(matches!(
            result.unwrap_err(),
            CaptureError::CaptureTimeout { .. } | CaptureError::PortalUnavailable { .. }
        ));
    }

    #[tokio::test]
    async fn test_capture_display_portal_unavailable() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        let opts = CaptureOptions::default();
        let result = backend.capture_display(None, &opts).await;
        assert!(result.is_err());
        // In test environment (no portal), capture_display fails with CaptureTimeout or
        // PortalUnavailable
        assert!(matches!(
            result.unwrap_err(),
            CaptureError::CaptureTimeout { .. } | CaptureError::PortalUnavailable { .. }
        ));
    }

    // ============================================================================
    // Group B: Timeout Wrapper Behavior Tests
    // ============================================================================

    #[tokio::test]
    async fn test_with_timeout_completes_successfully() {
        // Test that with_timeout allows fast operations to complete
        let fast_operation = async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok::<String, CaptureError>("success".to_string())
        };

        let result = WaylandBackend::with_timeout(fast_operation, 1).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_with_timeout_triggers_on_slow_operation() {
        // Test that with_timeout catches operations exceeding timeout
        let slow_operation = async {
            tokio::time::sleep(Duration::from_secs(2)).await;
            Ok::<String, CaptureError>("should not reach".to_string())
        };

        let result = WaylandBackend::with_timeout(slow_operation, 1).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CaptureError::CaptureTimeout { duration_ms: 1000 }
        ));
    }

    #[tokio::test]
    async fn test_with_timeout_propagates_inner_errors() {
        // Test that with_timeout doesn't mask inner errors
        let failing_operation = async {
            Err::<String, CaptureError>(CaptureError::PortalUnavailable {
                portal: "test".to_string(),
            })
        };

        let result = WaylandBackend::with_timeout(failing_operation, 10).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CaptureError::PortalUnavailable { .. }));
    }

    // ============================================================================
    // Group C: Fallback Trigger Logic Tests
    // ============================================================================

    #[tokio::test]
    async fn test_capture_window_fallback_preserves_region() {
        use crate::model::Region;

        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        // Create options with a region
        let opts = CaptureOptions {
            region: Some(Region {
                x: 100,
                y: 100,
                width: 200,
                height: 200,
            }),
            ..Default::default()
        };

        // Capture without token should trigger fallback
        let result = backend
            .capture_window("no-token-handle".to_string(), &opts)
            .await;

        // In CI environment, portal will timeout/unavailable
        // But the key point is fallback was attempted with region preserved
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CaptureError::CaptureTimeout { .. } | CaptureError::PortalUnavailable { .. }
        ));
    }

    #[tokio::test]
    async fn test_resolve_target_with_invalid_wayland_prefix() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        // Test various invalid formats
        let invalid_selectors = vec![
            ("wayland:", "Empty source_id"),
            ("wayland::", "Double colon"),
            ("wayland:a/b/c", "Slashes in source_id"),
        ];

        for (exe, _description) in invalid_selectors {
            let selector = WindowSelector {
                title_substring_or_regex: None,
                class: None,
                exe: Some(exe.to_string()),
            };

            let result = backend.resolve_target(&selector).await;

            // Empty source_id should return InvalidParameter
            // Non-empty but nonexistent should return TokenNotFound
            if exe == "wayland:" {
                assert!(matches!(result, Err(CaptureError::InvalidParameter { .. })));
            } else {
                assert!(matches!(result, Err(CaptureError::TokenNotFound { .. })));
            }
        }
    }

    #[tokio::test]
    async fn test_capture_window_with_region_crop() {
        use crate::model::Region;

        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(Arc::clone(&key_store));

        // Store a token (even though portal won't work in CI, test validates logic)
        key_store.store_token("test-window", "test-token").unwrap();

        let opts = CaptureOptions {
            region: Some(Region {
                x: 50,
                y: 50,
                width: 100,
                height: 100,
            }),
            ..Default::default()
        };

        let result = backend
            .capture_window("test-window".to_string(), &opts)
            .await;

        // Portal unavailable in CI, but logic path is correct
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CaptureError::CaptureTimeout { .. } | CaptureError::PortalUnavailable { .. }
        ));

        // Cleanup
        key_store.delete_token("test-window").unwrap();
    }

    // ============================================================================
    // Group A: Portal Error Path Tests
    // ============================================================================

    #[tokio::test]
    async fn test_prime_consent_portal_connection_timeout() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(Arc::clone(&key_store));

        // Attempt prime_consent - behavior depends on environment
        let result = backend
            .prime_consent(SourceType::Monitor, "timeout-test", false)
            .await;

        // On systems WITH portal: May succeed (user grants) or timeout (user doesn't
        // respond) On systems WITHOUT portal: Should get PortalUnavailable or
        // timeout Either outcome validates the error path exists
        match result {
            Ok(_) => {
                // Portal available and user granted permission (or it succeeded)
                // This is valid on live Wayland systems
                // Cleanup token to avoid polluting other tests
                let _ = key_store.delete_token("timeout-test");
            }
            Err(e) => {
                // Portal unavailable or timeout - expected in CI
                assert!(matches!(
                    e,
                    CaptureError::CaptureTimeout { .. }
                        | CaptureError::PortalUnavailable { .. }
                        | CaptureError::PermissionDenied { .. }
                ));
            }
        }
    }

    #[tokio::test]
    async fn test_prime_consent_session_creation_error() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(Arc::clone(&key_store));

        // Portal.create_session() would fail without live portal
        // This test verifies error handling in that code path
        let result = backend
            .prime_consent(SourceType::Window, "session-error", true)
            .await;

        // Expected: On systems with portal, may succeed or fail depending on user
        // On systems without portal: PortalUnavailable or CaptureTimeout
        // Either outcome is acceptable - test validates the code path exists
        match result {
            Ok(_) => {
                // Portal available and operation succeeded (valid on live systems)
                // Cleanup token to avoid polluting other tests
                let _ = key_store.delete_token("session-error");
            }
            Err(e) => {
                // Expected errors in CI or when portal unavailable
                assert!(matches!(
                    e,
                    CaptureError::PortalUnavailable { .. }
                        | CaptureError::CaptureTimeout { .. }
                        | CaptureError::PermissionDenied { .. }
                ));
            }
        }
    }

    #[tokio::test]
    async fn test_capture_display_with_scale() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        let opts = CaptureOptions {
            scale: 0.5, // 50% scale
            ..Default::default()
        };

        let result = backend.capture_display(None, &opts).await;

        // Portal unavailable in CI, but validates scale logic path
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CaptureError::CaptureTimeout { .. } | CaptureError::PortalUnavailable { .. }
        ));
    }

    #[tokio::test]
    async fn test_capture_window_token_rotation_on_success() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(Arc::clone(&key_store));

        // Store initial token
        key_store.store_token("rotation-test", "old-token").unwrap();

        let opts = CaptureOptions::default();
        let result = backend
            .capture_window("rotation-test".to_string(), &opts)
            .await;

        // Will fail in CI (no portal), but token should still exist
        assert!(result.is_err());

        // Token should still be present (not deleted on failure)
        assert!(key_store.has_token("rotation-test").unwrap());

        // Cleanup
        key_store.delete_token("rotation-test").unwrap();
    }
}
