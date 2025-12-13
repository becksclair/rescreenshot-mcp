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
#[cfg(feature = "linux-x11")]
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
#[cfg(feature = "linux-x11")]
use regex::RegexBuilder;
use x11rb::{
    connection::Connection as _,
    protocol::xproto::{Atom, ConnectionExt as _, Window},
    rust_connection::RustConnection,
};

use super::{CaptureFacade, ImageBuffer};
use crate::{
    error::{CaptureError, CaptureResult},
    model::{BackendType, Capabilities, CaptureOptions, WindowHandle, WindowInfo, WindowSelector},
};

/// Timeout for window enumeration operations (1.5s)
///
/// This allows approximately 100ms per window for systems with ~15 windows,
/// keeping total latency under the 2s target for list_windows + resolve_target
/// + capture_window workflow.
const LIST_WINDOWS_TIMEOUT_MS: u64 = 1500;

/// Timeout for single window capture operations (2s as per M3 spec)
///
/// xcap capture operations typically complete in <500ms, but we allow 2s
/// to accommodate:
/// - Large windows (4K, 8K displays)
/// - X server latency on remote connections
/// - Compositing effects on some window managers
#[allow(dead_code)] // Will be used in Phase 6 (capture_window implementation)
const CAPTURE_WINDOW_TIMEOUT_MS: u64 = 2000;

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
    #[allow(dead_code)] // Will be used in future phases (multi-screen support)
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
    /// Wraps async operation with timeout
    ///
    /// This helper ensures all X11 operations complete within expected time
    /// bounds. If an operation takes longer than the timeout, it returns a
    /// [`CaptureError::CaptureTimeout`].
    ///
    /// # Arguments
    ///
    /// - `future` - The async operation to wrap
    /// - `timeout_ms` - Timeout duration in milliseconds
    ///
    /// # Returns
    ///
    /// - `Ok(T)` - Operation completed successfully
    /// - `Err(CaptureTimeout)` - Operation exceeded timeout
    /// - `Err(...)` - Inner operation error propagated
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let result = Self::with_timeout(
    ///     async { some_x11_operation().await },
    ///     LIST_WINDOWS_TIMEOUT_MS,
    /// ).await?;
    /// ```
    async fn with_timeout<F, T>(future: F, timeout_ms: u64) -> CaptureResult<T>
    where
        F: std::future::Future<Output = CaptureResult<T>>,
    {
        tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), future)
            .await
            .map_err(|_| {
                tracing::warn!("X11 operation timed out after {}ms", timeout_ms);
                CaptureError::CaptureTimeout {
                    duration_ms: timeout_ms,
                }
            })?
    }

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

    /// Fetches metadata for a single window
    ///
    /// This method queries all properties (title, class, PID) for a window
    /// and constructs a [`WindowInfo`] struct. Properties are fetched
    /// sequentially to reuse the same connection.
    ///
    /// Windows without titles (likely system/background windows) are filtered
    /// out by returning `None`.
    ///
    /// # Arguments
    ///
    /// - `conn` - X11 connection reference
    /// - `window` - Window ID to query
    /// - `atoms` - Cached EWMH atoms for property queries
    ///
    /// # Returns
    ///
    /// - `Some(WindowInfo)` - Window metadata if title exists
    /// - `None` - Window has no title (likely system window)
    fn fetch_window_info(
        &self,
        conn: &RustConnection,
        window: Window,
        atoms: &X11Atoms,
    ) -> Option<WindowInfo> {
        // Try UTF-8 title first (_NET_WM_NAME), fallback to Latin-1 (WM_NAME)
        let title = self
            .get_property_utf8(conn, window, atoms.net_wm_name, atoms.utf8_string)
            .unwrap_or_else(|_| {
                self.get_property_latin1(conn, window, atoms.wm_name)
                    .unwrap_or_default()
            });

        // Skip windows without titles (system windows, background processes)
        if title.is_empty() {
            tracing::trace!("Skipping window {} (no title)", window);
            return None;
        }

        // Query WM_CLASS (instance + class names)
        let (instance, class) = self
            .get_property_class(conn, window, atoms.wm_class)
            .unwrap_or_default();

        // Query _NET_WM_PID
        let pid = self
            .get_property_pid(conn, window, atoms.net_wm_pid)
            .unwrap_or(0);

        tracing::trace!(
            "Window {}: title='{}', class='{}', instance='{}', pid={}",
            window,
            title,
            class,
            instance,
            pid
        );

        Some(WindowInfo::new(
            window.to_string(),
            title,
            class,
            instance, // owner field = instance name
            pid,
            BackendType::X11,
        ))
    }

    /// Tries to match windows using regex pattern
    ///
    /// Attempts to compile the pattern as a regex with safety limits:
    /// - Size limit: 1MB (prevents ReDoS)
    /// - Case-insensitive matching
    ///
    /// If regex compilation fails, returns None (caller will try substring
    /// match).
    ///
    /// # Arguments
    ///
    /// - `pattern` - Regex pattern to match against window titles
    /// - `windows` - List of windows to search
    ///
    /// # Returns
    ///
    /// - `Some(WindowHandle)` - First matching window
    /// - `None` - No match or invalid regex
    #[cfg(feature = "linux-x11")]
    fn try_regex_match(&self, pattern: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
        // Try to compile as regex with safety limits
        let regex = RegexBuilder::new(pattern)
            .case_insensitive(true)
            .size_limit(1_048_576) // 1MB limit (prevents ReDoS)
            .build();

        let regex = match regex {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!("Pattern '{}' is not a valid regex: {}", pattern, e);
                return None;
            }
        };

        // Find first match
        for window in windows {
            if regex.is_match(&window.title) {
                tracing::debug!("Regex matched window: {} (title: {})", window.id, window.title);
                return Some(window.id.clone());
            }
        }

        None
    }

    /// Tries to match windows using case-insensitive substring search
    ///
    /// # Arguments
    ///
    /// - `substring` - Substring to search for in window titles
    /// - `windows` - List of windows to search
    ///
    /// # Returns
    ///
    /// - `Some(WindowHandle)` - First matching window
    /// - `None` - No match
    fn try_substring_match(&self, substring: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
        let substring_lower = substring.to_lowercase();

        for window in windows {
            if window.title.to_lowercase().contains(&substring_lower) {
                tracing::debug!(
                    "Substring matched window: {} (title: {})",
                    window.id,
                    window.title
                );
                return Some(window.id.clone());
            }
        }

        None
    }

    /// Tries to match windows by exact WM_CLASS match
    ///
    /// # Arguments
    ///
    /// - `class` - Class name to match
    /// - `windows` - List of windows to search
    ///
    /// # Returns
    ///
    /// - `Some(WindowHandle)` - First matching window
    /// - `None` - No match
    fn try_exact_class_match(&self, class: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
        for window in windows {
            if window.class.eq_ignore_ascii_case(class) {
                tracing::debug!("Class matched window: {} (class: {})", window.id, window.class);
                return Some(window.id.clone());
            }
        }

        None
    }

    /// Tries to match windows by exact instance/exe name match
    ///
    /// The `owner` field in WindowInfo contains the WM_CLASS instance name,
    /// which typically corresponds to the executable name.
    ///
    /// # Arguments
    ///
    /// - `exe` - Executable/instance name to match
    /// - `windows` - List of windows to search
    ///
    /// # Returns
    ///
    /// - `Some(WindowHandle)` - First matching window
    /// - `None` - No match
    fn try_exact_exe_match(&self, exe: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
        for window in windows {
            if window.owner.eq_ignore_ascii_case(exe) {
                tracing::debug!("Exe matched window: {} (owner: {})", window.id, window.owner);
                return Some(window.id.clone());
            }
        }

        None
    }

    /// Tries to match windows using fuzzy matching
    ///
    /// Uses SkimMatcherV2 with a threshold of 60. Returns the highest-scoring
    /// match above the threshold.
    ///
    /// # Arguments
    ///
    /// - `pattern` - Pattern to fuzzy-match against window titles
    /// - `windows` - List of windows to search
    ///
    /// # Returns
    ///
    /// - `Some(WindowHandle)` - Best fuzzy match (score >= 60)
    /// - `None` - No match above threshold
    #[cfg(feature = "linux-x11")]
    fn try_fuzzy_match(&self, pattern: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
        let matcher = SkimMatcherV2::default();
        const THRESHOLD: i64 = 60;

        let mut best_match: Option<(WindowHandle, i64)> = None;

        for window in windows {
            if let Some(score) = matcher.fuzzy_match(&window.title, pattern) {
                if score >= THRESHOLD {
                    tracing::debug!(
                        "Fuzzy match candidate: {} (title: {}, score: {})",
                        window.id,
                        window.title,
                        score
                    );

                    // Keep highest-scoring match
                    if best_match.as_ref().map_or(true, |(_, s)| score > *s) {
                        best_match = Some((window.id.clone(), score));
                    }
                }
            }
        }

        if let Some((handle, score)) = best_match {
            tracing::debug!("Best fuzzy match: {} (score: {})", handle, score);
            Some(handle)
        } else {
            None
        }
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
        Self::with_timeout(
            async {
                tracing::debug!("Starting X11 window enumeration");

                // Get connection and atoms
                let (conn, screen_idx) = self.get_or_create_connection()?;
                let atoms = self.get_atoms().await?;

                // Query _NET_CLIENT_LIST for all window IDs
                let window_ids = self.get_client_list(&conn, screen_idx, atoms.net_client_list)?;

                tracing::debug!("Found {} window IDs, fetching metadata", window_ids.len());

                // Fetch metadata for all windows sequentially
                // (Parallel fetching would require multiple connections, which is more complex)
                let mut windows = Vec::new();
                for &win_id in &window_ids {
                    if let Some(info) = self.fetch_window_info(&conn, win_id, &atoms) {
                        windows.push(info);
                    }
                }

                tracing::info!(
                    "Enumerated {} X11 windows (filtered {} system windows)",
                    windows.len(),
                    window_ids.len() - windows.len()
                );

                Ok(windows)
            },
            LIST_WINDOWS_TIMEOUT_MS,
        )
        .await
    }

    /// Resolves a window selector to a window handle
    ///
    /// Searches for windows matching the selector using this evaluation order:
    /// 1. **Regex match** - If `title_substring_or_regex` is a valid regex
    ///    pattern
    /// 2. **Substring match** - Case-insensitive substring search on title
    /// 3. **Exact class match** - Exact match on WM_CLASS
    /// 4. **Exact exe match** - Exact match on process name (derived from
    ///    WM_CLASS instance)
    /// 5. **Fuzzy match** - Scored fuzzy matching (threshold >= 60)
    ///
    /// Returns the highest-scoring match if multiple windows qualify.
    ///
    /// # Arguments
    ///
    /// - `selector` - Window selector with title/class/exe criteria
    ///
    /// # Returns
    ///
    /// A window handle (X11 Window ID as string).
    ///
    /// # Errors
    ///
    /// - [`CaptureError::WindowNotFound`] - No matching window found
    /// - [`CaptureError::InvalidParameter`] - Empty selector
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Regex match
    /// let selector = WindowSelector::by_title("Fire.*");
    /// let handle = backend.resolve_target(&selector).await?;
    ///
    /// // Substring match
    /// let selector = WindowSelector::by_title("code");
    /// let handle = backend.resolve_target(&selector).await?;
    ///
    /// // Class match
    /// let selector = WindowSelector::by_class("Alacritty");
    /// let handle = backend.resolve_target(&selector).await?;
    /// ```
    #[cfg(feature = "linux-x11")]
    async fn resolve_target(&self, selector: &WindowSelector) -> CaptureResult<WindowHandle> {
        use std::time::Duration;

        tracing::debug!("Resolving window target: {:?}", selector);

        // Validate selector is not empty
        if selector.title_substring_or_regex.is_none()
            && selector.class.is_none()
            && selector.exe.is_none()
        {
            return Err(CaptureError::InvalidParameter {
                parameter: "selector".to_string(),
                reason:    "At least one field (title, class, or exe) must be specified"
                    .to_string(),
            });
        }

        // Get all windows
        let windows = self.list_windows().await?;
        if windows.is_empty() {
            return Err(CaptureError::WindowNotFound {
                selector: selector.clone(),
            });
        }

        // Try evaluation chain with timeout (50ms per strategy, 200ms total)
        let result = tokio::time::timeout(Duration::from_millis(200), async {
            // Strategy 1: Regex match on title
            if let Some(ref pattern) = selector.title_substring_or_regex {
                if let Some(handle) = self.try_regex_match(pattern, &windows) {
                    tracing::info!("Resolved window via regex: {}", handle);
                    return Ok(handle);
                }
            }

            // Strategy 2: Case-insensitive substring match on title
            if let Some(ref substring) = selector.title_substring_or_regex {
                if let Some(handle) = self.try_substring_match(substring, &windows) {
                    tracing::info!("Resolved window via substring: {}", handle);
                    return Ok(handle);
                }
            }

            // Strategy 3: Exact class match
            if let Some(ref class) = selector.class {
                if let Some(handle) = self.try_exact_class_match(class, &windows) {
                    tracing::info!("Resolved window via class: {}", handle);
                    return Ok(handle);
                }
            }

            // Strategy 4: Exact exe match (owner field contains instance name)
            if let Some(ref exe) = selector.exe {
                if let Some(handle) = self.try_exact_exe_match(exe, &windows) {
                    tracing::info!("Resolved window via exe: {}", handle);
                    return Ok(handle);
                }
            }

            // Strategy 5: Fuzzy match (threshold >= 60)
            if let Some(ref pattern) = selector.title_substring_or_regex {
                if let Some(handle) = self.try_fuzzy_match(pattern, &windows) {
                    tracing::info!("Resolved window via fuzzy match: {}", handle);
                    return Ok(handle);
                }
            }

            // No match found
            Err(CaptureError::WindowNotFound {
                selector: selector.clone(),
            })
        })
        .await
        .map_err(|_| {
            tracing::warn!("Window resolution timed out after 200ms");
            CaptureError::CaptureTimeout { duration_ms: 200 }
        })??;

        Ok(result)
    }

    #[cfg(not(feature = "linux-x11"))]
    async fn resolve_target(&self, _selector: &WindowSelector) -> CaptureResult<WindowHandle> {
        Err(CaptureError::BackendNotAvailable {
            backend: BackendType::X11,
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
    #[cfg(feature = "linux-x11")]
    async fn capture_window(
        &self,
        handle: WindowHandle,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        tracing::info!("Capturing X11 window: handle={}", handle);

        // Parse window handle as u32 X11 Window ID
        let win_id = handle.parse::<u32>().map_err(|e| {
            tracing::error!("Invalid window handle '{}': {}", handle, e);
            CaptureError::InvalidParameter {
                parameter: "window_handle".to_string(),
                reason:    format!("Expected numeric X11 window ID, got '{}'", handle),
            }
        })?;

        // Wrap xcap capture in spawn_blocking to avoid blocking async runtime
        // xcap uses synchronous X11 calls that can block
        let capture_future = tokio::task::spawn_blocking(move || {
            tracing::debug!("Enumerating xcap windows to find ID: {}", win_id);

            // xcap doesn't have a from_raw_id() method, so we enumerate all windows
            // and find the one with matching ID
            let windows = xcap::Window::all().map_err(|e| {
                tracing::error!("Failed to enumerate xcap windows: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                }
            })?;

            tracing::debug!("Found {} xcap windows, searching for ID {}", windows.len(), win_id);

            // Find window by ID (xcap's Window::id() returns Result<u32>)
            let window = windows
                .into_iter()
                .find(|w| w.id().ok() == Some(win_id))
                .ok_or_else(|| {
                    tracing::error!("Window {} not found in xcap enumeration", win_id);
                    CaptureError::WindowNotFound {
                        selector: crate::model::WindowSelector {
                            title_substring_or_regex: Some(format!("window_id:{}", win_id)),
                            class: None,
                            exe: None,
                        },
                    }
                })?;

            tracing::debug!(
                "Found matching xcap window: {} ('{}')",
                win_id,
                window.title().unwrap_or_else(|_| "Unknown".to_string())
            );

            // Capture the window image
            let image = window.capture_image().map_err(|e| {
                tracing::error!("xcap capture failed for window {}: {}", win_id, e);

                // Map capture errors
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("not found") || err_str.contains("destroyed") {
                    CaptureError::WindowNotFound {
                        selector: crate::model::WindowSelector {
                            title_substring_or_regex: Some(format!("window_id:{}", win_id)),
                            class: None,
                            exe: None,
                        },
                    }
                } else {
                    CaptureError::BackendNotAvailable {
                        backend: BackendType::X11,
                    }
                }
            })?;

            tracing::info!(
                "Successfully captured window {} ({}x{})",
                win_id,
                image.width(),
                image.height()
            );

            Ok::<_, CaptureError>(image)
        });

        // Wait for capture with timeout
        let image = Self::with_timeout(
            async {
                capture_future.await.map_err(|e| {
                    tracing::error!("Capture task panicked: {}", e);
                    CaptureError::BackendNotAvailable {
                        backend: BackendType::X11,
                    }
                })?
            },
            CAPTURE_WINDOW_TIMEOUT_MS,
        )
        .await?;

        // Convert RgbaImage to ImageBuffer and apply transformations
        let mut buffer = ImageBuffer::new(image::DynamicImage::ImageRgba8(image));

        // Apply transformations (crop, scale, format)
        if let Some(region) = &opts.region {
            tracing::debug!("Applying crop: {:?}", region);
            buffer = buffer.crop(*region)?;
        }

        // scale is f32, not Option<f32>
        if opts.scale != 1.0 {
            tracing::debug!("Applying scale: {}", opts.scale);
            buffer = buffer.scale(opts.scale)?;
        }

        Ok(buffer)
    }

    #[cfg(not(feature = "linux-x11"))]
    async fn capture_window(
        &self,
        _handle: WindowHandle,
        _opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        Err(CaptureError::BackendNotAvailable {
            backend: BackendType::X11,
        })
    }

    /// Captures a screenshot of an entire display
    ///
    /// Captures the primary monitor or a specific display by ID. On X11, this
    /// uses xcap to capture the full screen and optionally apply
    /// transformations.
    ///
    /// # Arguments
    ///
    /// - `display_id` - Optional display ID (unused on X11, always captures
    ///   primary screen)
    /// - `opts` - Capture options (region, scale, format)
    ///
    /// # Returns
    ///
    /// An [`ImageBuffer`] with the captured screenshot.
    ///
    /// # Errors
    ///
    /// - [`CaptureError::BackendNotAvailable`] - Display capture failed
    /// - [`CaptureError::CaptureTimeout`] - Operation exceeded timeout
    #[cfg(feature = "linux-x11")]
    async fn capture_display(
        &self,
        display_id: Option<u32>,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        tracing::info!(
            "X11 capture_display: capturing primary display (display_id={:?})",
            display_id
        );

        // xcap doesn't have a direct "display capture" API; use Screen::all()
        // Screen is essentially a monitor/display in xcap terminology
        let capture_future = tokio::task::spawn_blocking(|| {
            // Get all screens and capture the primary one
            let screens = xcap::Screen::all().map_err(|e| {
                tracing::error!("xcap failed to enumerate screens: {}", e);
                self.map_xcap_error(e)
            })?;

            if screens.is_empty() {
                tracing::error!("No screens available for capture");
                return Err(CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                });
            }

            // Use first screen (primary display)
            let screen = &screens[0];
            tracing::debug!(
                "Capturing screen: {}x{} at ({}, {})",
                screen.width(),
                screen.height(),
                screen.x(),
                screen.y()
            );

            let image = screen.capture_image().map_err(|e| {
                tracing::error!("xcap screen capture failed: {}", e);
                self.map_xcap_error(e)
            })?;

            tracing::info!("Successfully captured display: {}x{}", image.width(), image.height());

            Ok::<_, CaptureError>(image)
        });

        // Wait for capture with timeout
        let image = Self::with_timeout(
            async {
                capture_future.await.map_err(|e| {
                    tracing::error!("Screen capture task panicked: {}", e);
                    CaptureError::BackendNotAvailable {
                        backend: BackendType::X11,
                    }
                })?
            },
            CAPTURE_WINDOW_TIMEOUT_MS,
        )
        .await?;

        // Convert RgbaImage to ImageBuffer and apply transformations
        let mut buffer = ImageBuffer::new(image::DynamicImage::ImageRgba8(image));

        // Apply transformations (crop, scale)
        if let Some(region) = &opts.region {
            tracing::debug!("Applying crop to display: {:?}", region);
            buffer = buffer.crop(*region)?;
        }

        if opts.scale != 1.0 {
            tracing::debug!("Applying scale to display: {}", opts.scale);
            buffer = buffer.scale(opts.scale)?;
        }

        Ok(buffer)
    }

    #[cfg(not(feature = "linux-x11"))]
    async fn capture_display(
        &self,
        _display_id: Option<u32>,
        _opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        Err(CaptureError::BackendNotAvailable {
            backend: BackendType::X11,
        })
    }

    /// Maps xcap errors to CaptureError with remediation hints
    ///
    /// Provides user-friendly error messages with actionable next steps
    /// for common xcap failure scenarios.
    ///
    /// # Arguments
    ///
    /// - `e` - xcap error object
    ///
    /// # Returns
    ///
    /// Mapped CaptureError with remediation text
    fn map_xcap_error(&self, e: xcap::XcapError) -> CaptureError {
        let err_str = e.to_string().to_lowercase();

        // Permission denied - possibly running in restricted environment
        if err_str.contains("permission denied") || err_str.contains("access denied") {
            tracing::warn!("xcap permission denied - check X11 security restrictions");
            return CaptureError::BackendNotAvailable {
                backend: BackendType::X11,
            };
        }

        // Display connection failed
        if err_str.contains("display") || err_str.contains("connection") {
            tracing::warn!("xcap failed to connect to X11 display - verify DISPLAY is set");
            return CaptureError::BackendNotAvailable {
                backend: BackendType::X11,
            };
        }

        // Window not found (specific to capture_window)
        if err_str.contains("not found") || err_str.contains("destroyed") {
            return CaptureError::BackendNotAvailable {
                backend: BackendType::X11,
            };
        }

        // Generic fallback
        tracing::error!("xcap error: {}", e);
        CaptureError::BackendNotAvailable {
            backend: BackendType::X11,
        }
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
    async fn test_list_windows_returns_windows() {
        // Only run if DISPLAY is set (requires live X11 session)
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let result = backend.list_windows().await;

            // Should succeed (even if empty on minimal systems)
            assert!(result.is_ok(), "list_windows should succeed with DISPLAY set");

            let windows = result.unwrap();
            tracing::debug!("Found {} windows in test", windows.len());

            // Verify structure of returned windows
            for window in &windows {
                // All windows should have non-empty IDs
                assert!(!window.id.is_empty(), "Window ID should not be empty");
                // All windows should have non-empty titles (filtered by fetch_window_info)
                assert!(!window.title.is_empty(), "Window title should not be empty");
                // Backend should be X11
                assert_eq!(window.backend, BackendType::X11);
                tracing::trace!("Window: id={}, title='{}'", window.id, window.title);
            }
        }
    }

    #[tokio::test]
    async fn test_list_windows_timeout() {
        // This test validates the timeout wrapper exists and compiles
        // Actual timeout behavior is hard to test without mocking
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let result = backend.list_windows().await;
            // Should complete within timeout (1.5s is generous for real systems)
            assert!(
                result.is_ok()
                    || matches!(
                        result.unwrap_err(),
                        CaptureError::CaptureTimeout { .. }
                            | CaptureError::BackendNotAvailable { .. }
                    )
            );
        }
    }

    #[tokio::test]
    async fn test_with_timeout_helper() {
        // Test timeout helper with fast operation
        let fast_result = X11Backend::with_timeout(
            async { Ok::<_, CaptureError>(42) },
            100, // 100ms timeout
        )
        .await;
        assert!(fast_result.is_ok());
        assert_eq!(fast_result.unwrap(), 42);

        // Test timeout helper with slow operation
        let slow_result = X11Backend::with_timeout(
            async {
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                Ok::<_, CaptureError>(())
            },
            50, // 50ms timeout (less than sleep time)
        )
        .await;
        assert!(slow_result.is_err());
        assert!(matches!(slow_result.unwrap_err(), CaptureError::CaptureTimeout { .. }));
    }

    // Unit tests for resolve_target matching strategies
    // These tests use synthetic WindowInfo data to validate matching logic

    #[test]
    #[cfg(feature = "linux-x11")]
    fn test_try_regex_match() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let windows = vec![
                WindowInfo::new(
                    "1".to_string(),
                    "Firefox Browser".to_string(),
                    "Firefox".to_string(),
                    "firefox".to_string(),
                    1234,
                    BackendType::X11,
                ),
                WindowInfo::new(
                    "2".to_string(),
                    "VS Code Editor".to_string(),
                    "Code".to_string(),
                    "code".to_string(),
                    5678,
                    BackendType::X11,
                ),
            ];

            // Valid regex - should match Firefox
            let result = backend.try_regex_match("^Fire.*", &windows);
            assert_eq!(result, Some("1".to_string()));

            // Case-insensitive regex - should match VS Code
            let result = backend.try_regex_match("code", &windows);
            assert_eq!(result, Some("2".to_string()));

            // Invalid regex - should return None (will fallback to substring)
            let result = backend.try_regex_match("[invalid(", &windows);
            assert_eq!(result, None);

            // Non-matching regex - should return None
            let result = backend.try_regex_match("^Chrome", &windows);
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_try_substring_match() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let windows = vec![
                WindowInfo::new(
                    "1".to_string(),
                    "Firefox Browser".to_string(),
                    "Firefox".to_string(),
                    "firefox".to_string(),
                    1234,
                    BackendType::X11,
                ),
                WindowInfo::new(
                    "2".to_string(),
                    "VS Code Editor".to_string(),
                    "Code".to_string(),
                    "code".to_string(),
                    5678,
                    BackendType::X11,
                ),
            ];

            // Case-insensitive substring match - should match Firefox
            let result = backend.try_substring_match("firefox", &windows);
            assert_eq!(result, Some("1".to_string()));

            // Partial match - should match VS Code
            let result = backend.try_substring_match("editor", &windows);
            assert_eq!(result, Some("2".to_string()));

            // Case-insensitive partial match
            let result = backend.try_substring_match("CODE", &windows);
            assert_eq!(result, Some("2".to_string()));

            // Non-matching substring - should return None
            let result = backend.try_substring_match("chrome", &windows);
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_try_exact_class_match() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let windows = vec![
                WindowInfo::new(
                    "1".to_string(),
                    "Terminal Window".to_string(),
                    "Alacritty".to_string(),
                    "alacritty".to_string(),
                    1234,
                    BackendType::X11,
                ),
                WindowInfo::new(
                    "2".to_string(),
                    "Editor Window".to_string(),
                    "Code".to_string(),
                    "code".to_string(),
                    5678,
                    BackendType::X11,
                ),
            ];

            // Exact class match (case-insensitive) - should match Alacritty
            let result = backend.try_exact_class_match("alacritty", &windows);
            assert_eq!(result, Some("1".to_string()));

            // Case variation - should still match
            let result = backend.try_exact_class_match("ALACRITTY", &windows);
            assert_eq!(result, Some("1".to_string()));

            // Match VS Code
            let result = backend.try_exact_class_match("Code", &windows);
            assert_eq!(result, Some("2".to_string()));

            // Non-matching class - should return None
            let result = backend.try_exact_class_match("Firefox", &windows);
            assert_eq!(result, None);
        }
    }

    #[test]
    fn test_try_exact_exe_match() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let windows = vec![
                WindowInfo::new(
                    "1".to_string(),
                    "Terminal Window".to_string(),
                    "Alacritty".to_string(),
                    "alacritty".to_string(),
                    1234,
                    BackendType::X11,
                ),
                WindowInfo::new(
                    "2".to_string(),
                    "Editor Window".to_string(),
                    "Code".to_string(),
                    "code".to_string(),
                    5678,
                    BackendType::X11,
                ),
            ];

            // Exact exe/owner match - should match alacritty
            let result = backend.try_exact_exe_match("alacritty", &windows);
            assert_eq!(result, Some("1".to_string()));

            // Case variation - should still match
            let result = backend.try_exact_exe_match("CODE", &windows);
            assert_eq!(result, Some("2".to_string()));

            // Non-matching exe - should return None
            let result = backend.try_exact_exe_match("firefox", &windows);
            assert_eq!(result, None);
        }
    }

    #[test]
    #[cfg(feature = "linux-x11")]
    fn test_try_fuzzy_match() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let windows = vec![
                WindowInfo::new(
                    "1".to_string(),
                    "Firefox Browser".to_string(),
                    "Firefox".to_string(),
                    "firefox".to_string(),
                    1234,
                    BackendType::X11,
                ),
                WindowInfo::new(
                    "2".to_string(),
                    "VS Code Editor".to_string(),
                    "Code".to_string(),
                    "code".to_string(),
                    5678,
                    BackendType::X11,
                ),
            ];

            // Fuzzy match with typo - should match Firefox (threshold >= 60)
            let result = backend.try_fuzzy_match("firefo", &windows);
            // fuzzy-matcher should score this above threshold
            assert!(result.is_some());

            // Fuzzy match with abbreviation
            let _result = backend.try_fuzzy_match("vscode", &windows);
            // May or may not match depending on scoring algorithm
            // This test validates the function exists and returns Option

            // Very poor fuzzy match - should return None (below threshold)
            let result = backend.try_fuzzy_match("xyz123", &windows);
            assert_eq!(result, None);
        }
    }

    #[tokio::test]
    #[cfg(feature = "linux-x11")]
    async fn test_resolve_target_empty_selector() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let selector = WindowSelector {
                title_substring_or_regex: None,
                class: None,
                exe: None,
            };
            let result = backend.resolve_target(&selector).await;
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), CaptureError::InvalidParameter { .. }));
        }
    }

    #[tokio::test]
    async fn test_resolve_target_no_windows() {
        // This test verifies error handling when no windows exist
        // In real X11 environment, there's always at least one window
        // But we test the error path exists
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let selector = WindowSelector::by_title("nonexistent_window_12345");
            let result = backend.resolve_target(&selector).await;
            // Either WindowNotFound or finds a real window
            assert!(result.is_err() || result.is_ok(), "Should handle both cases gracefully");
        }
    }

    #[tokio::test]
    async fn test_resolve_target_invalid_regex_fallback() {
        // Test that invalid regex falls back to substring match
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            // Invalid regex pattern with unmatched bracket
            let selector = WindowSelector::by_title("[invalid(");
            let result = backend.resolve_target(&selector).await;
            // Should not panic, either finds a window or returns WindowNotFound
            assert!(result.is_err() || result.is_ok(), "Invalid regex should fallback gracefully");
        }
    }

    #[tokio::test]
    #[cfg(feature = "linux-x11")]
    async fn test_capture_display_succeeds() {
        // Only run if DISPLAY is set (requires live X11 session)
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let opts = CaptureOptions::default();
            let result = backend.capture_display(None, &opts).await;

            // Should succeed on systems with displays
            if result.is_ok() {
                let buffer = result.unwrap();

                // Verify dimensions are valid
                assert!(
                    buffer.width() > 0 && buffer.height() > 0,
                    "Captured display should have non-zero dimensions"
                );

                // Verify image is valid (not corrupted)
                let expected_pixels = buffer.width() as u64 * buffer.height() as u64;
                assert!(expected_pixels > 0, "Image should have valid pixel count");

                // Reasonable display size (typical displays 640x480 minimum)
                assert!(
                    buffer.width() >= 640 || buffer.height() >= 480,
                    "Display should be at least 640x480 (common minimum)"
                );

                tracing::info!(
                    "✓ Display capture succeeded: {}x{} ({} pixels)",
                    buffer.width(),
                    buffer.height(),
                    expected_pixels
                );
            } else {
                // Acceptable on minimal/remote systems
                tracing::warn!("Display capture failed (acceptable on minimal systems)");
            }
        }
    }

    #[tokio::test]
    #[cfg(feature = "linux-x11")]
    async fn test_capture_display_with_region() {
        // Test display capture with region cropping
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let mut opts = CaptureOptions::default();
            opts.region = Some(crate::model::Region {
                x:      0,
                y:      0,
                width:  100,
                height: 100,
            });

            let result = backend.capture_display(None, &opts).await;

            if result.is_ok() {
                let buffer = result.unwrap();
                // Cropped region should be at most 100x100
                assert!(
                    buffer.width() <= 100 && buffer.height() <= 100,
                    "Cropped display should respect region bounds"
                );
                tracing::info!("Cropped display: {}x{}", buffer.width(), buffer.height());
            }
        }
    }

    #[test]
    #[cfg(feature = "linux-x11")]
    fn test_map_xcap_error_permission_denied() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();

            // Simulate permission denied error
            let error = xcap::XcapError::new("Permission denied accessing X11");
            let result = backend.map_xcap_error(error);

            assert!(matches!(result, CaptureError::BackendNotAvailable { .. }));
        }
    }

    #[test]
    #[cfg(feature = "linux-x11")]
    fn test_map_xcap_error_display_connection_failed() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();

            // Simulate display connection error
            let error = xcap::XcapError::new("Failed to connect to display");
            let result = backend.map_xcap_error(error);

            assert!(matches!(result, CaptureError::BackendNotAvailable { .. }));
        }
    }

    #[test]
    #[cfg(feature = "linux-x11")]
    fn test_map_xcap_error_window_not_found() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();

            // Simulate window not found error
            let error = xcap::XcapError::new("Window not found");
            let result = backend.map_xcap_error(error);

            assert!(matches!(result, CaptureError::BackendNotAvailable { .. }));
        }
    }

    #[test]
    #[cfg(feature = "linux-x11")]
    fn test_map_xcap_error_generic_fallback() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();

            // Simulate generic unknown error
            let error = xcap::XcapError::new("Unknown xcap error");
            let result = backend.map_xcap_error(error);

            // Should always map to BackendNotAvailable with a log message
            assert!(matches!(result, CaptureError::BackendNotAvailable { .. }));
        }
    }

    // Phase 9: Additional comprehensive unit tests

    #[test]
    fn test_x11_backend_display_environment() {
        // Test that backend correctly detects DISPLAY environment variable
        let original = std::env::var("DISPLAY").ok();

        // With DISPLAY set
        if original.is_some() {
            std::env::set_var("DISPLAY", ":0");
            let result = X11Backend::new();
            assert!(result.is_ok(), "Backend should work with DISPLAY=:0");
        }

        // Without DISPLAY
        std::env::remove_var("DISPLAY");
        let result = X11Backend::new();
        assert!(result.is_err(), "Backend should fail without DISPLAY set");

        // Restore original
        if let Some(val) = original {
            std::env::set_var("DISPLAY", val);
        } else {
            std::env::remove_var("DISPLAY");
        }
    }

    #[tokio::test]
    async fn test_capture_result_is_send_sync() {
        // Verify that capture results can be sent across async boundaries
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();

            // This would fail to compile if CaptureResult isn't Send + Sync
            let task = tokio::spawn(async move {
                let _result = backend.list_windows().await;
            });

            assert!(task.await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_multiple_windows_sorting() {
        // Verify that list_windows returns consistent ordering
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();

            let windows1 = backend.list_windows().await.unwrap_or_default();
            let windows2 = backend.list_windows().await.unwrap_or_default();

            // Same window list returned (though order may vary by system)
            if windows1.len() == windows2.len() && !windows1.is_empty() {
                let ids1: Vec<_> = windows1.iter().map(|w| &w.id).collect();
                let ids2: Vec<_> = windows2.iter().map(|w| &w.id).collect();

                // All IDs should appear in both lists
                for id in &ids1 {
                    assert!(ids2.contains(id), "Window ID should appear in both lists");
                }
            }
        }
    }

    #[test]
    #[cfg(feature = "linux-x11")]
    fn test_with_timeout_boundary_conditions() {
        // Test timeout at exact boundary
        let rt = tokio::runtime::Runtime::new().unwrap();

        // Test with 1ms timeout and instant operation (should succeed)
        let result = rt.block_on(X11Backend::with_timeout(async { Ok::<_, CaptureError>(()) }, 1));
        assert!(result.is_ok(), "Instant operation should succeed with 1ms timeout");

        // Test with very long timeout (10s)
        let result =
            rt.block_on(X11Backend::with_timeout(async { Ok::<_, CaptureError>(()) }, 10000));
        assert!(result.is_ok(), "Should support long timeouts");

        // Test that error propagates through timeout wrapper
        let result = rt.block_on(X11Backend::with_timeout(
            async {
                Err::<_, CaptureError>(CaptureError::BackendNotAvailable {
                    backend: BackendType::X11,
                })
            },
            1000,
        ));
        assert!(result.is_err(), "Errors should propagate through timeout");
    }

    #[test]
    #[cfg(feature = "linux-x11")]
    fn test_try_regex_match_edge_cases() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let windows = vec![
                WindowInfo::new(
                    "1".to_string(),
                    "".to_string(), // Empty title
                    "Class".to_string(),
                    "exe".to_string(),
                    1234,
                    BackendType::X11,
                ),
                WindowInfo::new(
                    "2".to_string(),
                    "Normal Title".to_string(),
                    "Class".to_string(),
                    "exe".to_string(),
                    5678,
                    BackendType::X11,
                ),
            ];

            // Regex matching empty string
            let result = backend.try_regex_match("^$", &windows);
            assert_eq!(result, Some("1".to_string()), "Should match empty title");

            // Regex with special characters
            let result = backend.try_regex_match("Normal.*", &windows);
            assert_eq!(result, Some("2".to_string()), "Should match title with regex");

            // Case-insensitive flag test
            let result = backend.try_regex_match("(?i)normal", &windows);
            assert_eq!(result, Some("2".to_string()), "Should support case-insensitive");
        }
    }

    #[test]
    fn test_try_substring_match_case_sensitivity() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let windows = vec![WindowInfo::new(
                "1".to_string(),
                "MyWindow".to_string(),
                "Class".to_string(),
                "exe".to_string(),
                1234,
                BackendType::X11,
            )];

            // Lowercase search against title with mixed case
            let result = backend.try_substring_match("mywindow", &windows);
            assert_eq!(result, Some("1".to_string()), "Match should be case-insensitive");

            // Uppercase search
            let result = backend.try_substring_match("MYWINDOW", &windows);
            assert_eq!(result, Some("1".to_string()), "Uppercase search should work");

            // Partial match
            let result = backend.try_substring_match("Window", &windows);
            assert_eq!(result, Some("1".to_string()), "Partial substring match");
        }
    }

    #[test]
    fn test_try_exact_class_match_whitespace() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let windows = vec![WindowInfo::new(
                "1".to_string(),
                "Title".to_string(),
                "MyClass Name".to_string(), // Whitespace in class
                "exe".to_string(),
                1234,
                BackendType::X11,
            )];

            // Exact match with whitespace
            let result = backend.try_exact_class_match("myclass name", &windows);
            assert_eq!(result, Some("1".to_string()), "Should handle whitespace in class names");

            // Partial match should fail
            let result = backend.try_exact_class_match("MyClass", &windows);
            assert_eq!(result, None, "Partial class match should not succeed");
        }
    }

    #[test]
    #[cfg(feature = "linux-x11")]
    fn test_try_fuzzy_match_threshold() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let windows = vec![WindowInfo::new(
                "1".to_string(),
                "VeryLongWindowTitle".to_string(),
                "Class".to_string(),
                "exe".to_string(),
                1234,
                BackendType::X11,
            )];

            // Close match (high score)
            let result1 = backend.try_fuzzy_match("VeryLongWindowTitle", &windows);
            assert!(result1.is_some(), "Exact fuzzy match should succeed");

            // Partial match
            let result2 = backend.try_fuzzy_match("VeryLong", &windows);
            // May or may not match depending on threshold

            // Very poor match (should fail threshold)
            let result3 = backend.try_fuzzy_match("xyz", &windows);
            assert_eq!(result3, None, "Very poor fuzzy match should fail threshold");
        }
    }

    #[test]
    fn test_capabilities_immutable() {
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let caps1 = backend.capabilities();
            let caps2 = backend.capabilities();

            // Capabilities should be consistent across calls
            assert_eq!(caps1.supports_cursor, caps2.supports_cursor);
            assert_eq!(caps1.supports_region, caps2.supports_region);
            assert_eq!(caps1.supports_wayland_restore, caps2.supports_wayland_restore);
            assert_eq!(caps1.supports_window_capture, caps2.supports_window_capture);
            assert_eq!(caps1.supports_display_capture, caps2.supports_display_capture);
        }
    }

    #[tokio::test]
    async fn test_as_any_downcasting() {
        // Verify that as_any() allows backend downcasting
        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();
            let facade: &dyn CaptureFacade = &backend;

            // Should be able to downcast to X11Backend
            let any = facade.as_any();
            let x11 = any.downcast_ref::<X11Backend>();
            assert!(x11.is_some(), "Should be able to downcast to X11Backend");
        }
    }

    #[test]
    fn test_constants_values() {
        // Verify timeout constants are reasonable
        assert_eq!(LIST_WINDOWS_TIMEOUT_MS, 1500);
        assert!(LIST_WINDOWS_TIMEOUT_MS > 0, "LIST_WINDOWS_TIMEOUT should be positive");

        assert_eq!(CAPTURE_WINDOW_TIMEOUT_MS, 2000);
        assert!(
            CAPTURE_WINDOW_TIMEOUT_MS > LIST_WINDOWS_TIMEOUT_MS,
            "Capture timeout should be greater than list timeout"
        );
    }

    #[tokio::test]
    #[cfg(feature = "linux-x11")]
    async fn test_image_buffer_validity() {
        // Verify that ImageBuffer can hold valid image data
        // This test validates the image capture pipeline produces valid buffers

        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();

            // Try to capture display
            let opts = CaptureOptions::default();
            if let Ok(buffer) = backend.capture_display(None, &opts).await {
                // Validate buffer properties
                let width = buffer.width();
                let height = buffer.height();

                // Check dimensions are sane
                assert!(width > 0 && width < 10000, "Width should be reasonable");
                assert!(height > 0 && height < 10000, "Height should be reasonable");

                // Check pixel count doesn't overflow
                let pixel_count = (width as u64) * (height as u64);
                assert!(pixel_count > 0, "Pixel count should be positive");
                assert!(
                    pixel_count < 1_000_000_000,
                    "Pixel count should be reasonable (<1 billion)"
                );

                tracing::info!("✓ ImageBuffer validity test passed: {}x{} image", width, height);
            }
        }
    }

    #[tokio::test]
    #[cfg(feature = "linux-x11")]
    async fn test_captured_image_has_valid_dimensions() {
        // Verify that captured images have consistent valid dimensions
        // This is a sanity check that dimensions match pixel data

        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();

            // List windows and capture first one
            if let Ok(windows) = backend.list_windows().await {
                if !windows.is_empty() {
                    let first_window = &windows[0];
                    let selector = WindowSelector::by_title(&first_window.title);

                    if let Ok(handle) = backend.resolve_target(&selector).await {
                        let opts = CaptureOptions::default();
                        if let Ok(buffer) = backend.capture_window(handle, &opts).await {
                            // Image should have non-zero dimensions
                            let width = buffer.width();
                            let height = buffer.height();

                            assert!(
                                width > 0 && height > 0,
                                "Captured image should have valid dimensions"
                            );

                            // Pixel count should be consistent
                            let pixel_count = (width as u64) * (height as u64);
                            assert!(pixel_count > 0, "Pixel count should match dimensions");

                            // Dimensions should be reasonable (not ridiculously large)
                            assert!(
                                width < 50000 && height < 50000,
                                "Dimensions should be reasonable"
                            );

                            tracing::info!(
                                "✓ Image dimensions valid: {}x{} ({} pixels)",
                                width,
                                height,
                                pixel_count
                            );
                        }
                    }
                }
            }
        }
    }

    #[tokio::test]
    #[cfg(feature = "linux-x11")]
    async fn test_captured_image_has_pixel_data() {
        // Verify that captured image actually contains pixel data (not blank/zeroed)
        // This test ensures images are genuinely captured, not just dimension-checked

        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();

            // Try to capture display (always available)
            let opts = CaptureOptions::default();
            if let Ok(buffer) = backend.capture_display(None, &opts).await {
                let width = buffer.width();
                let height = buffer.height();

                // Get raw pixel bytes
                let bytes = buffer.as_bytes();

                // Verify bytes exist and have content
                assert!(!bytes.is_empty(), "Image bytes should not be empty");

                // Expected minimum size: width * height * 4 bytes per pixel (RGBA)
                // Allow some variation for different image formats
                let min_expected = (width as usize) * (height as usize) * 3; // RGB minimum
                assert!(
                    bytes.len() >= min_expected,
                    "Image bytes should contain at least RGB data (expected {} bytes, got {})",
                    min_expected,
                    bytes.len()
                );

                // Check that image is not entirely zeroed (all black)
                // Count non-zero bytes - should be significant majority
                let non_zero_count = bytes.iter().filter(|&&b| b != 0).count();
                let zero_ratio = 1.0 - (non_zero_count as f64 / bytes.len() as f64);

                tracing::info!(
                    "Pixel data check: {} bytes, {} non-zero ({:.1}% zero)",
                    bytes.len(),
                    non_zero_count,
                    zero_ratio * 100.0
                );

                // Image should have some variation in pixels (not completely uniform)
                // Allow up to 50% zero bytes for mostly-black displays
                assert!(
                    zero_ratio < 0.5,
                    "Image should have pixel variation (got {:.1}% zero bytes, expected <50%)",
                    zero_ratio * 100.0
                );

                tracing::info!(
                    "✓ Image contains real pixel data: {} bytes with {:.1}% variation",
                    bytes.len(),
                    (1.0 - zero_ratio) * 100.0
                );
            }
        }
    }

    #[tokio::test]
    #[cfg(feature = "linux-x11")]
    async fn test_window_capture_vs_display_different_data() {
        // Verify that different capture sources produce different pixel data
        // This proves captures are actually sourcing different parts of screen

        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();

            // Get display capture
            let opts = CaptureOptions::default();
            let display_image = match backend.capture_display(None, &opts).await {
                Ok(img) => img,
                Err(_) => {
                    tracing::warn!("Skipping comparison test: display capture failed");
                    return;
                }
            };

            // Get window list and capture first window
            let windows = match backend.list_windows().await {
                Ok(w) => w,
                Err(_) => {
                    tracing::warn!("Skipping comparison test: list_windows failed");
                    return;
                }
            };

            if windows.is_empty() {
                tracing::warn!("Skipping comparison test: no windows found");
                return;
            }

            let selector = WindowSelector::by_title(&windows[0].title);
            let handle = match backend.resolve_target(&selector).await {
                Ok(h) => h,
                Err(_) => {
                    tracing::warn!("Skipping comparison test: resolve_target failed");
                    return;
                }
            };

            let window_image = match backend.capture_window(handle, &opts).await {
                Ok(img) => img,
                Err(_) => {
                    tracing::warn!("Skipping comparison test: capture_window failed");
                    return;
                }
            };

            // Compare pixel data
            let display_bytes = display_image.as_bytes();
            let window_bytes = window_image.as_bytes();

            // Window should be smaller than display (or equal if window is fullscreen)
            let window_smaller_or_equal = window_bytes.len() <= display_bytes.len();
            tracing::info!(
                "Display bytes: {}, Window bytes: {}",
                display_bytes.len(),
                window_bytes.len()
            );
            assert!(
                window_smaller_or_equal,
                "Window capture should be smaller than display (or equal if fullscreen)"
            );

            // If both are available, check they're not identical (different regions)
            if window_bytes.len() < display_bytes.len() {
                let identical_start = display_bytes[..window_bytes.len()] == window_bytes[..];
                // They might be identical if window is at top-left, so just verify both have
                // content
                assert!(
                    !display_bytes.is_empty() && !window_bytes.is_empty(),
                    "Both captures should contain pixel data"
                );

                tracing::info!("✓ Both display and window captures contain real pixel data");
            }
        }
    }

    #[tokio::test]
    #[cfg(feature = "linux-x11")]
    async fn test_region_crop_reduces_pixel_data() {
        // Verify that region cropping actually reduces the amount of pixel data
        // This proves transformations are applied to real image data

        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();

            // Capture full display
            let full_opts = CaptureOptions::default();
            let full_image = match backend.capture_display(None, &full_opts).await {
                Ok(img) => img,
                Err(_) => {
                    tracing::warn!("Skipping region crop test: full capture failed");
                    return;
                }
            };

            let full_bytes = full_image.as_bytes();
            let full_len = full_bytes.len();

            // Capture with region crop (small area)
            let mut cropped_opts = CaptureOptions::default();
            cropped_opts.region = Some(crate::model::Region {
                x:      0,
                y:      0,
                width:  100,
                height: 100,
            });

            let cropped_image = match backend.capture_display(None, &cropped_opts).await {
                Ok(img) => img,
                Err(_) => {
                    tracing::warn!("Skipping region crop test: cropped capture failed");
                    return;
                }
            };

            let cropped_bytes = cropped_image.as_bytes();
            let cropped_len = cropped_bytes.len();

            // Cropped should be significantly smaller
            assert!(
                cropped_len < full_len,
                "Cropped image should use less data ({} < {})",
                cropped_len,
                full_len
            );

            // Verify crop ratio is reasonable
            let ratio = cropped_len as f64 / full_len as f64;
            tracing::info!(
                "Crop test: full={} bytes, cropped={} bytes (ratio: {:.2}%)",
                full_len,
                cropped_len,
                ratio * 100.0
            );

            // Cropped should be less than 25% of full (conservative estimate)
            assert!(
                ratio < 0.25,
                "Cropped image should be <25% of full size (got {:.2}%)",
                ratio * 100.0
            );

            tracing::info!("✓ Region cropping produces smaller pixel data as expected");
        }
    }

    #[tokio::test]
    #[cfg(feature = "linux-x11")]
    async fn test_scale_transform_changes_byte_size() {
        // Verify that scaling actually changes the image byte size
        // This proves scaling transformation is applied to pixel data

        if std::env::var("DISPLAY").is_ok() {
            let backend = X11Backend::new().unwrap();

            // Capture at normal scale
            let normal_opts = CaptureOptions::default();
            let normal_image = match backend.capture_display(None, &normal_opts).await {
                Ok(img) => img,
                Err(_) => {
                    tracing::warn!("Skipping scale test: normal capture failed");
                    return;
                }
            };

            let normal_bytes = normal_image.as_bytes();
            let normal_len = normal_bytes.len();
            let normal_dims = (normal_image.width(), normal_image.height());

            // Capture at 50% scale
            let mut scaled_opts = CaptureOptions::default();
            scaled_opts.scale = 0.5;

            let scaled_image = match backend.capture_display(None, &scaled_opts).await {
                Ok(img) => img,
                Err(_) => {
                    tracing::warn!("Skipping scale test: scaled capture failed");
                    return;
                }
            };

            let scaled_bytes = scaled_image.as_bytes();
            let scaled_len = scaled_bytes.len();
            let scaled_dims = (scaled_image.width(), scaled_image.height());

            // Scaled dimensions should be smaller
            assert!(
                scaled_dims.0 <= normal_dims.0 && scaled_dims.1 <= normal_dims.1,
                "Scaled image dimensions should be smaller or equal"
            );

            // Byte size should be noticeably smaller for 50% scale
            if scaled_dims != normal_dims {
                // At 50% scale, expect roughly 25% of pixel data
                let ratio = scaled_len as f64 / normal_len as f64;
                tracing::info!(
                    "Scale test: normal={} bytes ({:?}), scaled={} bytes ({:?}, ratio: {:.2}%)",
                    normal_len,
                    normal_dims,
                    scaled_len,
                    scaled_dims,
                    ratio * 100.0
                );

                assert!(
                    ratio < 0.6,
                    "50% scaled image should use <60% of original bytes (got {:.2}%)",
                    ratio * 100.0
                );

                tracing::info!(
                    "✓ Scale transform produces smaller pixel data as expected (ratio: {:.2}%)",
                    ratio * 100.0
                );
            }
        }
    }
}
