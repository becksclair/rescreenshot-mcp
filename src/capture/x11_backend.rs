//! X11 capture backend using x11rb + xcap
//!
//! This module implements screenshot capture for X11 display servers. It
//! provides:
//!
//! - **Window Enumeration**: Query _NET_CLIENT_LIST for window metadata
//! - **Fuzzy Matching**: Regex, substring, and fuzzy-match window selection
//! - **Direct Capture**: Fast window capture via xcap library
//! - **Connection Management**: Lazy shared connection with reconnect-on-error
//!
//! # Architecture
//!
//! - **Stateless Backend**: Only stores shared X11 connection + cached atoms
//! - **Lazy Connection**: Connection created on first use, shared via
//!   Arc<Mutex<>>
//! - **Atom Caching**: EWMH atoms interned once and cached via OnceCell
//! - **Thread-Safe**: All operations are async-safe and thread-safe
//!
//! # X11 Security Model
//!
//! X11 allows direct window enumeration and capture without explicit user
//! permission. This backend queries EWMH (_NET) properties for window metadata
//! and uses xcap for screenshot capture.
//!
//! # Examples
//!
//! ```rust,ignore
//! use screenshot_mcp::{
//!     capture::{CaptureFacade, x11_backend::X11Backend},
//!     model::{CaptureOptions, WindowSelector},
//! };
//!
//! #[tokio::main]
//! async fn main() {
//!     let backend = X11Backend::new().unwrap();
//!
//!     // List all windows
//!     let windows = backend.list_windows().await.unwrap();
//!
//!     // Capture by title
//!     let selector = WindowSelector::by_title("Firefox");
//!     let handle = backend.resolve_target(&selector).await.unwrap();
//!     let opts = CaptureOptions::default();
//!     let image = backend.capture_window(handle, &opts).await.unwrap();
//! }
//! ```

use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use x11rb::{
    connection::Connection as _,
    protocol::xproto::{Atom, AtomEnum, ConnectionExt as _, Window},
    rust_connection::RustConnection,
};

use super::{CaptureFacade, ImageBuffer};
use crate::{
    error::{CaptureError, CaptureResult},
    model::{BackendType, Capabilities, CaptureOptions, WindowHandle, WindowInfo, WindowSelector},
};

/// X11 screenshot backend using x11rb + xcap
///
/// Implements the [`CaptureFacade`] trait for X11 display servers. Uses a lazy
/// shared connection to the X server and caches EWMH atoms for efficient
/// property queries.
///
/// # Thread Safety
///
/// `X11Backend` is fully thread-safe (`Send + Sync`) and can be shared across
/// tasks using `Arc`. The internal X11 connection is protected by a mutex and
/// lazy-initialized on first use.
///
/// # Connection Management
///
/// The connection is created lazily on first use and shared across all
/// operations. If the connection fails, it will attempt to reconnect once
/// before failing.
#[derive(Debug)]
pub struct X11Backend {
    /// Lazy shared X11 connection (reconnect-on-error)
    conn:       Arc<Mutex<Option<RustConnection>>>,
    /// Screen index (typically 0 for default screen)
    screen_idx: usize,
    /// Cached EWMH atoms (initialized once on first use)
    atoms:      OnceLock<X11Atoms>,
}

/// Cached EWMH atoms for efficient property queries
///
/// These atoms are interned once during backend initialization and reused
/// for all property queries. This avoids repeated atom interning overhead.
#[derive(Debug, Clone)]
struct X11Atoms {
    /// _NET_CLIENT_LIST: list of all managed windows
    net_client_list: Atom,
    /// _NET_WM_NAME: UTF-8 encoded window title
    net_wm_name:     Atom,
    /// WM_NAME: Latin-1 encoded window title (fallback)
    wm_name:         Atom,
    /// WM_CLASS: Window class/instance names
    wm_class:        Atom,
    /// _NET_WM_PID: Process ID owning the window
    net_wm_pid:      Atom,
    /// UTF8_STRING: atom for UTF-8 text encoding
    utf8_string:     Atom,
}

impl X11Backend {
    /// Creates a new X11Backend instance
    ///
    /// The connection is not established until the first operation. This allows
    /// the backend to be created even if the X server is not yet available.
    ///
    /// # Returns
    ///
    /// - `Ok(X11Backend)` - Backend created successfully
    /// - `Err(BackendNotAvailable)` - $DISPLAY not set or invalid
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use screenshot_mcp::capture::x11_backend::X11Backend;
    ///
    /// let backend = X11Backend::new().unwrap();
    /// ```
    pub fn new() -> CaptureResult<Self> {
        // Check if $DISPLAY is set
        if std::env::var("DISPLAY").is_err() {
            return Err(CaptureError::BackendNotAvailable {
                backend: BackendType::X11,
            });
        }

        Ok(Self {
            conn:       Arc::new(Mutex::new(None)),
            screen_idx: 0,
            atoms:      OnceLock::new(),
        })
    }

    /// Gets or creates a shared X11 connection
    ///
    /// This method implements lazy initialization with reconnect-on-error:
    /// 1. If connection exists and is valid, return reference to it
    /// 2. If connection doesn't exist, create and cache it
    /// 3. If operation fails, clear cached connection to force reconnect on
    ///    next call
    ///
    /// # Returns
    ///
    /// - `Ok(&RustConnection)` - Connection reference (valid until mutex is
    ///   released)
    /// - `Err(BackendNotAvailable)` - Failed to connect to X server
    ///
    /// # Thread Safety
    ///
    /// This method acquires a mutex lock. The connection is valid only while
    /// the lock guard is held. Callers should minimize lock hold time.
    fn get_or_create_connection(&self) -> CaptureResult<(RustConnection, usize)> {
        let mut conn_guard = self.conn.lock().unwrap();

        // If connection exists, check if it's still valid by attempting a no-op
        if let Some(existing_conn) = conn_guard.as_ref() {
            // Test connection with a lightweight query (get input focus)
            // This fails fast if the connection is broken
            if existing_conn.get_input_focus().is_ok() {
                tracing::trace!("Reusing existing X11 connection");
                // Cannot return reference to connection while holding lock
                // Must create new connection for now
                // TODO: Optimize this in future by using Arc<RustConnection>
                // instead
            } else {
                tracing::warn!("X11 connection is stale, reconnecting");
                *conn_guard = None;
            }
        }

        // Create new connection if needed
        if conn_guard.is_none() {
            tracing::debug!("Creating new X11 connection");
            match x11rb::connect(None) {
                Ok((new_conn, new_screen_idx)) => {
                    tracing::debug!("X11 connection established (screen {})", new_screen_idx);

                    // Store screen_idx for later use
                    // For Phase 2, we'll just use the new connection directly
                    // and recreate it each time. Phase 3 will optimize this.

                    // Store connection
                    *conn_guard = Some(new_conn);

                    // We need to return the connection, but we can't return a reference
                    // while holding the lock. For Phase 2, we'll just create a new one.
                    // This will be optimized in Phase 3 by restructuring the connection storage.
                    drop(conn_guard);

                    // Create a fresh connection to return
                    // (Since RustConnection doesn't implement Clone)
                    let (conn, screen_idx) = x11rb::connect(None).map_err(|e| {
                        tracing::error!("Failed to reconnect to X11: {}", e);
                        CaptureError::BackendNotAvailable {
                            backend: BackendType::X11,
                        }
                    })?;
                    return Ok((conn, screen_idx));
                }
                Err(e) => {
                    tracing::error!("Failed to connect to X11: {}", e);
                    return Err(CaptureError::BackendNotAvailable {
                        backend: BackendType::X11,
                    });
                }
            }
        }

        // If we reach here, connection exists and is valid, but we need to return
        // a new connection since we can't return a reference while holding the lock.
        // This is a temporary limitation that will be resolved in Phase 3.
        drop(conn_guard);

        let (conn, screen_idx) = x11rb::connect(None).map_err(|e| {
            tracing::error!("Failed to connect to X11: {}", e);
            CaptureError::BackendNotAvailable {
                backend: BackendType::X11,
            }
        })?;
        Ok((conn, screen_idx))
    }

    /// Interns EWMH atoms in batch
    ///
    /// This method queries all required atoms in a single batch operation to
    /// minimize round-trips to the X server. Atoms are cached in `self.atoms`
    /// for reuse across operations.
    ///
    /// # Returns
    ///
    /// - `Ok(X11Atoms)` - Atoms interned successfully
    /// - `Err(BackendNotAvailable)` - Atom interning failed
    async fn intern_atoms(&self, conn: &RustConnection) -> CaptureResult<X11Atoms> {
        use x11rb::protocol::xproto::*;

        // Intern atoms in batch (single round-trip)
        let net_client_list = conn.intern_atom(false, b"_NET_CLIENT_LIST").map_err(|e| {
            tracing::error!("Failed to intern _NET_CLIENT_LIST: {}", e);
            CaptureError::BackendNotAvailable {
                backend: BackendType::X11,
            }
        })?;

        let net_wm_name = conn.intern_atom(false, b"_NET_WM_NAME").map_err(|e| {
            tracing::error!("Failed to intern _NET_WM_NAME: {}", e);
            CaptureError::BackendNotAvailable {
                backend: BackendType::X11,
            }
        })?;

        let wm_name = conn.intern_atom(false, b"WM_NAME").map_err(|e| {
            tracing::error!("Failed to intern WM_NAME: {}", e);
            CaptureError::BackendNotAvailable {
                backend: BackendType::X11,
            }
        })?;

        let wm_class = conn.intern_atom(false, b"WM_CLASS").map_err(|e| {
            tracing::error!("Failed to intern WM_CLASS: {}", e);
            CaptureError::BackendNotAvailable {
                backend: BackendType::X11,
            }
        })?;

        let net_wm_pid = conn.intern_atom(false, b"_NET_WM_PID").map_err(|e| {
            tracing::error!("Failed to intern _NET_WM_PID: {}", e);
            CaptureError::BackendNotAvailable {
                backend: BackendType::X11,
            }
        })?;

        let utf8_string = conn.intern_atom(false, b"UTF8_STRING").map_err(|e| {
            tracing::error!("Failed to intern UTF8_STRING: {}", e);
            CaptureError::BackendNotAvailable {
                backend: BackendType::X11,
            }
        })?;

        // Collect replies
        let atoms = X11Atoms {
            net_client_list: net_client_list
                .reply()
                .map_err(|e| {
                    tracing::error!("Failed to get _NET_CLIENT_LIST reply: {}", e);
                    CaptureError::BackendNotAvailable {
                        backend: BackendType::X11,
                    }
                })?
                .atom,
            net_wm_name:     net_wm_name
                .reply()
                .map_err(|e| {
                    tracing::error!("Failed to get _NET_WM_NAME reply: {}", e);
                    CaptureError::BackendNotAvailable {
                        backend: BackendType::X11,
                    }
                })?
                .atom,
            wm_name:         wm_name
                .reply()
                .map_err(|e| {
                    tracing::error!("Failed to get WM_NAME reply: {}", e);
                    CaptureError::BackendNotAvailable {
                        backend: BackendType::X11,
                    }
                })?
                .atom,
            wm_class:        wm_class
                .reply()
                .map_err(|e| {
                    tracing::error!("Failed to get WM_CLASS reply: {}", e);
                    CaptureError::BackendNotAvailable {
                        backend: BackendType::X11,
                    }
                })?
                .atom,
            net_wm_pid:      net_wm_pid
                .reply()
                .map_err(|e| {
                    tracing::error!("Failed to get _NET_WM_PID reply: {}", e);
                    CaptureError::BackendNotAvailable {
                        backend: BackendType::X11,
                    }
                })?
                .atom,
            utf8_string:     utf8_string
                .reply()
                .map_err(|e| {
                    tracing::error!("Failed to get UTF8_STRING reply: {}", e);
                    CaptureError::BackendNotAvailable {
                        backend: BackendType::X11,
                    }
                })?
                .atom,
        };

        tracing::debug!("Interned EWMH atoms successfully");
        Ok(atoms)
    }

    /// Gets or initializes cached atoms
    ///
    /// Returns cached atoms if available, otherwise interns them using
    /// a fresh X11 connection.
    ///
    /// # Returns
    ///
    /// - `Ok(X11Atoms)` - Cached or newly interned atoms
    /// - `Err(BackendNotAvailable)` - Connection or interning failed
    async fn get_atoms(&self) -> CaptureResult<X11Atoms> {
        if let Some(atoms) = self.atoms.get() {
            tracing::trace!("Returning cached EWMH atoms");
            return Ok(atoms.clone());
        }

        tracing::debug!("Interning EWMH atoms for first time");
        let (conn, _screen_idx) = self.get_or_create_connection()?;
        let atoms = self.intern_atoms(&conn).await?;
        let _ = self.atoms.set(atoms.clone());
        Ok(atoms)
    }

    /// Queries a UTF-8 string property from a window
    ///
    /// Used for querying _NET_WM_NAME and other UTF-8 encoded properties.
    /// Returns empty string if property doesn't exist or contains invalid
    /// UTF-8.
    ///
    /// # Arguments
    ///
    /// - `conn` - X11 connection reference
    /// - `window` - Window to query
    /// - `property` - Property atom to read
    /// - `utf8_string` - UTF8_STRING atom for type check
    ///
    /// # Returns
    ///
    /// - `Ok(String)` - Property value (empty if not found/invalid)
    /// - `Err(BackendNotAvailable)` - Query failed
    fn get_property_utf8(
        &self,
        conn: &RustConnection,
        window: Window,
        property: Atom,
        utf8_string: Atom,
    ) -> CaptureResult<String> {
        use x11rb::protocol::xproto::*;

        // Query property (32KB limit to prevent DoS)
        let reply = conn
            .get_property(
                false,
                window,
                property,
                utf8_string,
                0,
                8192, // 32KB limit
            )
            .map_err(|e| {
                tracing::debug!("Failed to query UTF-8 property: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                }
            })?
            .reply()
            .map_err(|e| {
                tracing::debug!("Failed to get UTF-8 property reply: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                }
            })?;

        // Convert bytes to UTF-8 string (lossy conversion for safety)
        Ok(String::from_utf8_lossy(&reply.value).into_owned())
    }

    /// Queries a Latin-1 string property from a window
    ///
    /// Used for querying WM_NAME and other Latin-1 encoded properties
    /// (fallback). Returns empty string if property doesn't exist.
    ///
    /// # Arguments
    ///
    /// - `conn` - X11 connection reference
    /// - `window` - Window to query
    /// - `property` - Property atom to read
    ///
    /// # Returns
    ///
    /// - `Ok(String)` - Property value (empty if not found)
    /// - `Err(BackendNotAvailable)` - Query failed
    fn get_property_latin1(
        &self,
        conn: &RustConnection,
        window: Window,
        property: Atom,
    ) -> CaptureResult<String> {
        use x11rb::protocol::xproto::*;

        // Query property (32KB limit)
        let reply = conn
            .get_property(
                false,
                window,
                property,
                AtomEnum::STRING,
                0,
                8192, // 32KB limit
            )
            .map_err(|e| {
                tracing::debug!("Failed to query Latin-1 property: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                }
            })?
            .reply()
            .map_err(|e| {
                tracing::debug!("Failed to get Latin-1 property reply: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                }
            })?;

        // Latin-1 to UTF-8 conversion
        let s: String = reply.value.iter().map(|&b| b as char).collect();
        Ok(s)
    }

    /// Queries the PID property from a window
    ///
    /// Used for querying _NET_WM_PID.
    /// Returns 0 if property doesn't exist or is invalid.
    ///
    /// # Arguments
    ///
    /// - `conn` - X11 connection reference
    /// - `window` - Window to query
    /// - `property` - _NET_WM_PID atom
    ///
    /// # Returns
    ///
    /// - `Ok(u32)` - Process ID (0 if not found/invalid)
    /// - `Err(BackendNotAvailable)` - Query failed
    fn get_property_pid(
        &self,
        conn: &RustConnection,
        window: Window,
        property: Atom,
    ) -> CaptureResult<u32> {
        use x11rb::protocol::xproto::*;

        // Query property (CARDINAL type)
        let reply = conn
            .get_property(
                false,
                window,
                property,
                AtomEnum::CARDINAL,
                0,
                1, // PID is a single 32-bit value
            )
            .map_err(|e| {
                tracing::debug!("Failed to query PID property: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                }
            })?
            .reply()
            .map_err(|e| {
                tracing::debug!("Failed to get PID property reply: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                }
            })?;

        // Parse PID from bytes (32-bit little-endian)
        if reply.value.len() >= 4 {
            let pid = u32::from_ne_bytes([
                reply.value[0],
                reply.value[1],
                reply.value[2],
                reply.value[3],
            ]);
            Ok(pid)
        } else {
            Ok(0)
        }
    }

    /// Queries WM_CLASS property from a window
    ///
    /// WM_CLASS contains two null-terminated strings: instance name and class
    /// name. Returns (instance, class) tuple. Either may be empty if not
    /// found.
    ///
    /// # Arguments
    ///
    /// - `conn` - X11 connection reference
    /// - `window` - Window to query
    /// - `property` - WM_CLASS atom
    ///
    /// # Returns
    ///
    /// - `Ok((String, String))` - (instance, class) tuple
    /// - `Err(BackendNotAvailable)` - Query failed
    fn get_property_class(
        &self,
        conn: &RustConnection,
        window: Window,
        property: Atom,
    ) -> CaptureResult<(String, String)> {
        use x11rb::protocol::xproto::*;

        // Query property (STRING type)
        let reply = conn
            .get_property(
                false,
                window,
                property,
                AtomEnum::STRING,
                0,
                8192, // 32KB limit
            )
            .map_err(|e| {
                tracing::debug!("Failed to query WM_CLASS property: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                }
            })?
            .reply()
            .map_err(|e| {
                tracing::debug!("Failed to get WM_CLASS property reply: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                }
            })?;

        // WM_CLASS is two null-separated Latin-1 strings
        let bytes = &reply.value;
        let parts: Vec<String> = bytes
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| s.iter().map(|&b| b as char).collect())
            .collect();

        let instance = parts.first().cloned().unwrap_or_default();
        let class = parts.get(1).cloned().unwrap_or_default();

        Ok((instance, class))
    }

    /// Queries _NET_CLIENT_LIST from the root window
    ///
    /// Returns a list of all managed window IDs.
    ///
    /// # Arguments
    ///
    /// - `conn` - X11 connection reference
    /// - `screen_idx` - Screen index
    /// - `property` - _NET_CLIENT_LIST atom
    ///
    /// # Returns
    ///
    /// - `Ok(Vec<Window>)` - List of window IDs
    /// - `Err(BackendNotAvailable)` - Query failed
    fn get_client_list(
        &self,
        conn: &RustConnection,
        screen_idx: usize,
        property: Atom,
    ) -> CaptureResult<Vec<Window>> {
        use x11rb::protocol::xproto::*;

        // Get root window
        let root = conn.setup().roots[screen_idx].root;

        // Query _NET_CLIENT_LIST property
        let reply = conn
            .get_property(
                false,
                root,
                property,
                AtomEnum::WINDOW,
                0,
                4096, // Up to 16KB of window IDs
            )
            .map_err(|e| {
                tracing::error!("Failed to query _NET_CLIENT_LIST: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                }
            })?
            .reply()
            .map_err(|e| {
                tracing::error!("Failed to get _NET_CLIENT_LIST reply: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                }
            })?;

        // Parse window IDs from bytes (array of 32-bit values)
        let mut windows = Vec::new();
        for chunk in reply.value.chunks_exact(4) {
            let id = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            windows.push(id);
        }

        tracing::debug!("Found {} windows in _NET_CLIENT_LIST", windows.len());
        Ok(windows)
    }
}

#[async_trait]
impl CaptureFacade for X11Backend {
    /// Lists all X11 windows with metadata
    ///
    /// Queries the _NET_CLIENT_LIST property from the root window to enumerate
    /// all managed windows, then fetches properties (title, class, PID) for
    /// each.
    ///
    /// # Returns
    ///
    /// A vector of [`WindowInfo`] structs with window metadata.
    ///
    /// # Errors
    ///
    /// - [`CaptureError::BackendNotAvailable`] - X11 connection failed
    /// - [`CaptureError::CaptureTimeout`] - Operation exceeded 1.5s
    async fn list_windows(&self) -> CaptureResult<Vec<WindowInfo>> {
        tracing::info!("X11 list_windows called (stub)");
        // TODO: Implement in Phase 4
        Ok(vec![])
    }

    /// Resolves a window selector to a window handle
    ///
    /// Searches for windows matching the selector using:
    /// 1. Regex match (if pattern is valid regex)
    /// 2. Substring match (case-insensitive)
    /// 3. Fuzzy match (scored, threshold >50)
    ///
    /// # Returns
    ///
    /// A window handle (X11 Window ID as string).
    ///
    /// # Errors
    ///
    /// - [`CaptureError::WindowNotFound`] - No matching window found
    /// - [`CaptureError::InvalidParameter`] - Invalid regex pattern
    async fn resolve_target(&self, selector: &WindowSelector) -> CaptureResult<WindowHandle> {
        tracing::info!("X11 resolve_target called (stub): {:?}", selector);
        // TODO: Implement in Phase 5
        Err(CaptureError::WindowNotFound {
            selector: selector.clone(),
        })
    }

    /// Captures a screenshot of a specific window
    ///
    /// Uses xcap to capture the window by its X11 Window ID. Applies
    /// transformations (crop, scale) as specified in options.
    ///
    /// # Returns
    ///
    /// An [`ImageBuffer`] with the captured screenshot.
    ///
    /// # Errors
    ///
    /// - [`CaptureError::WindowNotFound`] - Window no longer exists
    /// - [`CaptureError::BackendNotAvailable`] - xcap capture failed
    /// - [`CaptureError::CaptureTimeout`] - Operation exceeded 2s
    async fn capture_window(
        &self,
        handle: WindowHandle,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        tracing::info!("X11 capture_window called (stub): handle={}, opts={:?}", handle, opts);
        // TODO: Implement in Phase 6
        Err(CaptureError::BackendNotAvailable {
            backend: BackendType::X11,
        })
    }

    /// Captures a screenshot of an entire display
    ///
    /// Captures the primary monitor or a specific display by ID.
    ///
    /// # Returns
    ///
    /// An [`ImageBuffer`] with the captured screenshot.
    ///
    /// # Errors
    ///
    /// - [`CaptureError::BackendNotAvailable`] - Display capture failed
    /// - [`CaptureError::InvalidParameter`] - Invalid display ID
    async fn capture_display(
        &self,
        display_id: Option<u32>,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        tracing::info!(
            "X11 capture_display called (stub): display_id={:?}, opts={:?}",
            display_id,
            opts
        );
        // TODO: Implement in Phase 7
        Err(CaptureError::BackendNotAvailable {
            backend: BackendType::X11,
        })
    }

    /// Returns the capabilities of this X11 backend
    ///
    /// X11 backends support:
    /// - Display capture: Yes
    /// - Window capture: Yes (direct enumeration)
    /// - Cursor inclusion: No (xcap limitation)
    /// - Region cropping: Yes (post-capture)
    /// - Restore tokens: No (X11 doesn't need permission persistence)
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_cursor:          false, // xcap doesn't support cursor capture
            supports_region:          true,  // Post-capture cropping supported
            supports_wayland_restore: false, // X11 doesn't use restore tokens
            supports_window_capture:  true,  // Direct enumeration allowed
            supports_display_capture: true,  // xcap supports display capture
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
    fn test_x11_backend_new_without_display() {
        // Temporarily unset DISPLAY
        let original = std::env::var("DISPLAY").ok();
        std::env::remove_var("DISPLAY");

        let result = X11Backend::new();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CaptureError::BackendNotAvailable { .. }));

        // Restore DISPLAY
        if let Some(val) = original {
            std::env::set_var("DISPLAY", val);
        }
    }

    #[test]
    fn test_x11_backend_new_with_display() {
        // Only run if DISPLAY is set
        if std::env::var("DISPLAY").is_ok() {
            let result = X11Backend::new();
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_capabilities() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let caps = backend.capabilities();

            assert!(!caps.supports_cursor); // xcap limitation
            assert!(caps.supports_region);
            assert!(!caps.supports_wayland_restore); // X11-specific
            assert!(caps.supports_window_capture);
            assert!(caps.supports_display_capture);
        }
    }

    #[tokio::test]
    async fn test_list_windows_stub() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let result = backend.list_windows().await;
            // Stub returns empty vec
            assert!(result.is_ok());
            assert_eq!(result.unwrap().len(), 0);
        }
    }

    #[tokio::test]
    async fn test_resolve_target_stub() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let selector = WindowSelector::by_title("test");
            let result = backend.resolve_target(&selector).await;
            // Stub returns WindowNotFound
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), CaptureError::WindowNotFound { .. }));
        }
    }
}
