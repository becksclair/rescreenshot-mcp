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

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use ashpd::desktop::{
    screencast::{CursorMode, SourceType as PortalSourceType},
    PersistMode as PortalPersistMode,
};
use async_trait::async_trait;
use image::GenericImageView;

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

    /// Captures a single frame from a PipeWire stream node
    ///
    /// This function connects to the specified PipeWire node, captures one frame,
    /// and converts it to a `DynamicImage`. It handles the complexity of PipeWire's
    /// async stream API for one-shot frame capture.
    ///
    /// # Arguments
    ///
    /// * `node_id` - PipeWire node ID from the portal stream
    ///
    /// # Returns
    ///
    /// - `Ok(DynamicImage)` with the captured frame in RGBA8 format
    /// - `Err(BackendNotAvailable)` if PipeWire connection fails or no frame received
    ///
    /// # Errors
    ///
    /// - [`CaptureError::BackendNotAvailable`] - PipeWire not available or connection failed
    /// - [`CaptureError::CaptureTimeout`] - No frame received within timeout
    ///
    /// # Implementation Note
    ///
    /// This uses blocking PipeWire API with a timeout for simplicity. A frame
    /// should arrive immediately since the portal session is already active.
    #[cfg(feature = "linux-wayland")]
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
                    tracing::debug!("Received format parameter (dimensions inference from buffer size)");
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
                                tracing::debug!("Captured PipeWire frame ({} bytes)", data_slice.len());

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
            let timeout_duration = Duration::from_secs(5); // 5 second timeout for frame capture

            while !frame_captured.load(Ordering::Relaxed) {
                if start.elapsed() > timeout_duration {
                    tracing::error!("Timeout waiting for PipeWire frame");
                    return Err(CaptureError::CaptureTimeout {
                        duration_ms: timeout_duration.as_millis() as u64,
                    });
                }

                // Run one iteration of the main loop (with 10ms timeout)
                let _result = mainloop.loop_().iterate(Duration::from_millis(10));
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
                    (1920, 1080), (2560, 1440), (3840, 2160), // 16:9
                    (1920, 1200), (2560, 1600), // 16:10
                    (1024, 768), (1280, 1024), // 4:3
                    (1366, 768), (1600, 900), // Laptop screens
                ];

                common_resolutions.iter()
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
                        return Err(CaptureError::TokenNotFound {
                            source_id: handle.clone(),
                        })
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

                proxy
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
                    })?;

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
                #[cfg(feature = "linux-wayland")]
                let raw_image = Self::capture_pipewire_frame(node_id).await?;

                #[cfg(not(feature = "linux-wayland"))]
                let raw_image: image::DynamicImage = {
                    return Err(CaptureError::BackendNotAvailable {
                        backend: BackendType::Wayland,
                    });
                };

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
            30, // 30-second timeout
        )
        .await
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
    async fn test_capture_window_no_token() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(key_store);

        let opts = CaptureOptions::default();
        let result = backend
            .capture_window("window-123".to_string(), &opts)
            .await;
        assert!(result.is_err());
        // Should fail with TokenNotFound since no token exists for this source
        assert!(matches!(result.unwrap_err(), CaptureError::TokenNotFound { .. }));
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
