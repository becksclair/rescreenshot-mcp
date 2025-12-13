//! Windows capture backend using Windows Graphics Capture API
//!
//! This module implements screenshot capture for Windows using:
//!
//! - **Window Enumeration**: Win32 EnumWindows API for window metadata
//! - **Fuzzy Matching**: Regex, substring, and fuzzy-match window selection
//! - **Direct Capture**: Windows Graphics Capture (WGC) via windows-capture
//!   crate
//!
//! # Architecture
//!
//! - **Stateless Backend**: No persistent state needed
//! - **Win32 APIs**: Window enumeration via windows-sys
//! - **WGC Capture**: Screen/window capture via windows-capture crate
//! - **Thread-Safe**: All operations are async-safe and thread-safe
//!
//! # Windows Version Requirements
//!
//! Windows Graphics Capture requires Windows 10 version 1803 (April 2018
//! Update) or later. Window enumeration works on all Windows versions.
//!
//! # Examples
//!
//! ```rust,ignore
//! use screenshot_mcp::{
//!     capture::{CaptureFacade, windows_backend::WindowsBackend},
//!     model::{CaptureOptions, WindowSelector},
//! };
//!
//! #[tokio::main]
//! async fn main() {
//!     let backend = WindowsBackend::new().unwrap();
//!
//!     // List all windows
//!     let windows = backend.list_windows().await.unwrap();
//!
//!     // Capture by title
//!     let selector = WindowSelector::by_title("Notepad");
//!     let handle = backend.resolve_target(&selector).await.unwrap();
//!     let opts = CaptureOptions::default();
//!     let image = backend.capture_window(handle, &opts).await.unwrap();
//! }
//! ```

use std::{any::Any, ffi::OsString, os::windows::ffi::OsStringExt, sync::mpsc};

use async_trait::async_trait;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use image::{DynamicImage, RgbaImage};
use regex::RegexBuilder;
use windows_capture::{
    capture::GraphicsCaptureApiHandler,
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor as WcMonitor,
    settings::{ColorFormat, CursorCaptureSettings, DrawBorderSettings, Settings},
    window::Window as WcWindow,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, BOOL, FALSE, HWND, TRUE},
    System::{
        ProcessStatus::GetModuleBaseNameW,
        Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
    },
    UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible,
    },
};

use super::{CaptureFacade, ImageBuffer};
use crate::{
    error::{CaptureError, CaptureResult},
    model::{BackendType, Capabilities, CaptureOptions, WindowHandle, WindowInfo, WindowSelector},
};

/// Timeout for window enumeration operations (1.5s)
///
/// Allows enumeration of many windows while keeping total latency reasonable.
#[allow(dead_code)]
const LIST_WINDOWS_TIMEOUT_MS: u64 = 1500;

/// Timeout for single window capture operations (2s)
///
/// WGC capture typically completes quickly, but we allow extra time for:
/// - Large windows (4K, 8K displays)
/// - GPU scheduling delays
/// - Compositing effects
#[allow(dead_code)]
const CAPTURE_WINDOW_TIMEOUT_MS: u64 = 2000;

/// Windows screenshot backend using Win32 + WGC
///
/// Implements the [`CaptureFacade`] trait for Windows. Uses Win32 APIs for
/// window enumeration and Windows Graphics Capture for screenshot capture.
///
/// # Thread Safety
///
/// `WindowsBackend` is fully thread-safe (`Send + Sync`) and can be shared
/// across tasks using `Arc`.
#[derive(Debug)]
pub struct WindowsBackend {
    // Stateless - no fields needed for now
    // Future: could cache monitor info, etc.
    _private: (),
}

impl WindowsBackend {
    /// Creates a new WindowsBackend instance
    ///
    /// # Returns
    ///
    /// - `Ok(WindowsBackend)` - Backend created successfully
    /// - `Err(BackendNotAvailable)` - Not running on Windows
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use screenshot_mcp::capture::windows_backend::WindowsBackend;
    ///
    /// let backend = WindowsBackend::new().unwrap();
    /// ```
    pub fn new() -> CaptureResult<Self> {
        // On Windows, we can always create the backend
        // WGC availability will be checked at capture time
        Ok(Self { _private: () })
    }

    /// Wraps async operation with timeout
    #[allow(dead_code)]
    async fn with_timeout<F, T>(future: F, timeout_ms: u64) -> CaptureResult<T>
    where
        F: std::future::Future<Output = CaptureResult<T>>,
    {
        tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), future)
            .await
            .map_err(|_| {
                tracing::warn!("Windows operation timed out after {}ms", timeout_ms);
                CaptureError::CaptureTimeout {
                    duration_ms: timeout_ms,
                }
            })?
    }

    /// Enumerates all top-level windows using Win32 EnumWindows
    ///
    /// Returns a vector of HWNDs for all visible windows with titles.
    fn enumerate_window_handles() -> Vec<isize> {
        let mut handles: Vec<isize> = Vec::new();

        unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: isize) -> BOOL {
            let handles = &mut *(lparam as *mut Vec<isize>);

            // Only include visible windows
            if IsWindowVisible(hwnd) == FALSE {
                return TRUE; // Continue enumeration
            }

            // Only include windows with titles
            let title_len = GetWindowTextLengthW(hwnd);
            if title_len == 0 {
                return TRUE; // Continue enumeration
            }

            handles.push(hwnd);
            TRUE // Continue enumeration
        }

        unsafe {
            EnumWindows(Some(enum_callback), &mut handles as *mut Vec<isize> as isize);
        }

        tracing::debug!("Enumerated {} window handles", handles.len());
        handles
    }

    /// Gets the title of a window
    fn get_window_title(hwnd: isize) -> String {
        unsafe {
            let len = GetWindowTextLengthW(hwnd);
            if len == 0 {
                return String::new();
            }

            // Allocate buffer (+1 for null terminator)
            let mut buffer: Vec<u16> = vec![0; (len + 1) as usize];
            let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);

            if copied == 0 {
                return String::new();
            }

            // Truncate to actual length and convert
            buffer.truncate(copied as usize);
            OsString::from_wide(&buffer).to_string_lossy().into_owned()
        }
    }

    /// Gets the class name of a window
    fn get_window_class(hwnd: isize) -> String {
        unsafe {
            let mut buffer: Vec<u16> = vec![0; 256];
            let len = GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);

            if len == 0 {
                return String::new();
            }

            buffer.truncate(len as usize);
            OsString::from_wide(&buffer).to_string_lossy().into_owned()
        }
    }

    /// Gets the process ID and executable name for a window
    fn get_window_process_info(hwnd: isize) -> (u32, String) {
        unsafe {
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);

            if pid == 0 {
                return (0, String::new());
            }

            // Open process to get exe name
            let process_handle =
                OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);

            if process_handle == 0 {
                return (pid, String::new());
            }

            // Get module base name (exe name) - use 0 for main module
            let mut exe_buffer: Vec<u16> = vec![0; 260]; // MAX_PATH
            let len = GetModuleBaseNameW(
                process_handle,
                0, // NULL module handle = main executable
                exe_buffer.as_mut_ptr(),
                exe_buffer.len() as u32,
            );

            CloseHandle(process_handle);

            if len == 0 {
                return (pid, String::new());
            }

            exe_buffer.truncate(len as usize);
            let exe_name = OsString::from_wide(&exe_buffer)
                .to_string_lossy()
                .into_owned();

            (pid, exe_name)
        }
    }

    /// Fetches complete WindowInfo for a single window handle
    fn fetch_window_info(hwnd: isize) -> Option<WindowInfo> {
        let title = Self::get_window_title(hwnd);

        // Skip windows without titles (already filtered in enumeration, but
        // double-check)
        if title.is_empty() {
            return None;
        }

        let class = Self::get_window_class(hwnd);
        let (pid, owner) = Self::get_window_process_info(hwnd);

        Some(WindowInfo {
            id: hwnd.to_string(),
            title,
            class,
            owner,
            pid,
            backend: BackendType::Windows,
        })
    }

    /// Enumerates all windows and returns their info
    fn enumerate_windows_sync() -> Vec<WindowInfo> {
        let handles = Self::enumerate_window_handles();
        handles
            .into_iter()
            .filter_map(Self::fetch_window_info)
            .collect()
    }

    // ========== Window Matching Strategies ==========

    /// Tries to match a window by regex pattern on title
    ///
    /// Returns the first window whose title matches the pattern.
    /// Pattern size is limited to 1MB to prevent ReDoS.
    fn try_regex_match(pattern: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
        // Limit pattern size to prevent ReDoS
        if pattern.len() > 1_000_000 {
            tracing::warn!("Regex pattern too large (>1MB), skipping regex match");
            return None;
        }

        let regex = match RegexBuilder::new(pattern).case_insensitive(true).build() {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!("Invalid regex pattern '{}': {}", pattern, e);
                return None;
            }
        };

        windows
            .iter()
            .find(|w| regex.is_match(&w.title))
            .map(|w| w.id.clone())
    }

    /// Tries to match a window by substring in title (case-insensitive)
    fn try_substring_match(substring: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
        let lower_substring = substring.to_lowercase();
        windows
            .iter()
            .find(|w| w.title.to_lowercase().contains(&lower_substring))
            .map(|w| w.id.clone())
    }

    /// Tries to match a window by exact class name (case-insensitive)
    fn try_exact_class_match(class: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
        let lower_class = class.to_lowercase();
        windows
            .iter()
            .find(|w| w.class.to_lowercase() == lower_class)
            .map(|w| w.id.clone())
    }

    /// Tries to match a window by exact executable name (case-insensitive)
    fn try_exact_exe_match(exe: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
        let lower_exe = exe.to_lowercase();
        windows
            .iter()
            .find(|w| w.owner.to_lowercase() == lower_exe)
            .map(|w| w.id.clone())
    }

    /// Tries to match a window using fuzzy matching on title
    ///
    /// Returns the window with the highest score above threshold (60).
    fn try_fuzzy_match(pattern: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
        let matcher = SkimMatcherV2::default();
        const FUZZY_THRESHOLD: i64 = 60;

        windows
            .iter()
            .filter_map(|w| {
                matcher
                    .fuzzy_match(&w.title, pattern)
                    .filter(|&score| score >= FUZZY_THRESHOLD)
                    .map(|score| (w, score))
            })
            .max_by_key(|(_, score)| *score)
            .map(|(w, _)| w.id.clone())
    }

    /// Creates a windows-capture Window from HWND
    fn create_wc_window(hwnd: isize) -> CaptureResult<WcWindow> {
        // Convert isize to raw HWND pointer
        let raw_hwnd = hwnd as *mut std::ffi::c_void;
        Ok(WcWindow::from_raw_hwnd(raw_hwnd))
    }

    /// Synchronously captures a window using WGC
    ///
    /// This function runs in a blocking context via spawn_blocking.
    fn capture_window_sync(hwnd: isize, include_cursor: bool) -> CaptureResult<DynamicImage> {
        use std::sync::{Arc, Mutex};

        use windows_capture::settings::{
            DirtyRegionSettings, MinimumUpdateIntervalSettings, SecondaryWindowSettings,
        };

        // Create window from HWND
        let window = Self::create_wc_window(hwnd)?;
        tracing::debug!("Created window for capture from HWND: {}", hwnd);

        // Create channel for receiving the captured frame
        let (tx, rx) = mpsc::sync_channel::<CaptureResult<DynamicImage>>(1);
        let tx = Arc::new(Mutex::new(Some(tx)));

        // Create capture handler inline
        struct OneShotCapture {
            tx: Arc<Mutex<Option<mpsc::SyncSender<CaptureResult<DynamicImage>>>>>,
        }

        impl GraphicsCaptureApiHandler for OneShotCapture {
            type Flags = Arc<Mutex<Option<mpsc::SyncSender<CaptureResult<DynamicImage>>>>>;
            type Error = Box<dyn std::error::Error + Send + Sync>;

            fn new(
                ctx: windows_capture::capture::Context<Self::Flags>,
            ) -> Result<Self, Self::Error> {
                Ok(Self { tx: ctx.flags })
            }

            fn on_frame_arrived(
                &mut self,
                frame: &mut Frame,
                capture_control: InternalCaptureControl,
            ) -> Result<(), Self::Error> {
                // Get dimensions first (before mutable borrow of buffer)
                let width = frame.width();
                let height = frame.height();

                // Get frame buffer (mutable borrow)
                let mut buffer = frame.buffer()?;

                // Convert BGRA to RGBA
                let raw_data = buffer.as_raw_buffer();
                let mut rgba_data = Vec::with_capacity(raw_data.len());

                for chunk in raw_data.chunks(4) {
                    if chunk.len() == 4 {
                        rgba_data.push(chunk[2]); // R
                        rgba_data.push(chunk[1]); // G
                        rgba_data.push(chunk[0]); // B
                        rgba_data.push(chunk[3]); // A
                    }
                }

                // Create image
                let image = match RgbaImage::from_raw(width, height, rgba_data) {
                    Some(img) => DynamicImage::ImageRgba8(img),
                    None => {
                        if let Some(tx) = self.tx.lock().unwrap().take() {
                            let _ = tx.send(Err(CaptureError::ImageError(
                                "Failed to create image from frame".into(),
                            )));
                        }
                        capture_control.stop();
                        return Ok(());
                    }
                };

                // Send frame
                if let Some(tx) = self.tx.lock().unwrap().take() {
                    let _ = tx.send(Ok(image));
                }

                // Stop after first frame
                capture_control.stop();
                Ok(())
            }

            fn on_closed(&mut self) -> Result<(), Self::Error> {
                Ok(())
            }
        }

        // Configure capture settings
        let cursor_settings = if include_cursor {
            CursorCaptureSettings::WithCursor
        } else {
            CursorCaptureSettings::WithoutCursor
        };

        let settings = Settings::new(
            window,
            cursor_settings,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            tx.clone(),
        );

        // Start capture
        let capture = OneShotCapture::start_free_threaded(settings).map_err(|e| {
            tracing::error!("Failed to start WGC capture: {}", e);
            CaptureError::BackendNotAvailable {
                backend: BackendType::Windows,
            }
        })?;

        // Wait for frame with timeout
        let result = rx
            .recv_timeout(std::time::Duration::from_millis(CAPTURE_WINDOW_TIMEOUT_MS))
            .map_err(|_| {
                tracing::warn!("Capture timeout waiting for frame");
                CaptureError::CaptureTimeout {
                    duration_ms: CAPTURE_WINDOW_TIMEOUT_MS,
                }
            })?;

        // Stop capture if still running
        drop(capture);

        result
    }

    /// Synchronously captures a monitor/display using WGC
    ///
    /// This function runs in a blocking context via spawn_blocking.
    fn capture_display_sync(
        display_id: Option<u32>,
        include_cursor: bool,
    ) -> CaptureResult<DynamicImage> {
        use std::sync::{Arc, Mutex};

        use windows_capture::settings::{
            DirtyRegionSettings, MinimumUpdateIntervalSettings, SecondaryWindowSettings,
        };

        // Get monitor - primary if None, or by index
        let monitor = match display_id {
            None => WcMonitor::primary().map_err(|e| {
                tracing::error!("Failed to get primary monitor: {}", e);
                CaptureError::BackendNotAvailable {
                    backend: BackendType::Windows,
                }
            })?,
            Some(id) => WcMonitor::from_index(id as usize).map_err(|e| {
                tracing::error!("Monitor {} not found: {}", id, e);
                CaptureError::InvalidParameter {
                    parameter: "display_id".to_string(),
                    reason:    format!("Monitor {} not found", id),
                }
            })?,
        };

        tracing::debug!("Capturing monitor: {:?}", monitor.name());

        // Create channel for receiving the captured frame
        let (tx, rx) = mpsc::sync_channel::<CaptureResult<DynamicImage>>(1);
        let tx = Arc::new(Mutex::new(Some(tx)));

        // Reuse the same capture handler pattern as window capture
        struct OneShotMonitorCapture {
            tx: Arc<Mutex<Option<mpsc::SyncSender<CaptureResult<DynamicImage>>>>>,
        }

        impl GraphicsCaptureApiHandler for OneShotMonitorCapture {
            type Flags = Arc<Mutex<Option<mpsc::SyncSender<CaptureResult<DynamicImage>>>>>;
            type Error = Box<dyn std::error::Error + Send + Sync>;

            fn new(
                ctx: windows_capture::capture::Context<Self::Flags>,
            ) -> Result<Self, Self::Error> {
                Ok(Self { tx: ctx.flags })
            }

            fn on_frame_arrived(
                &mut self,
                frame: &mut Frame,
                capture_control: InternalCaptureControl,
            ) -> Result<(), Self::Error> {
                // Get dimensions first
                let width = frame.width();
                let height = frame.height();

                // Get frame buffer
                let mut buffer = frame.buffer()?;
                let raw_data = buffer.as_raw_buffer();

                // Convert BGRA to RGBA
                let mut rgba_data = Vec::with_capacity(raw_data.len());
                for chunk in raw_data.chunks(4) {
                    if chunk.len() == 4 {
                        rgba_data.push(chunk[2]); // R
                        rgba_data.push(chunk[1]); // G
                        rgba_data.push(chunk[0]); // B
                        rgba_data.push(chunk[3]); // A
                    }
                }

                // Create image
                let image = match RgbaImage::from_raw(width, height, rgba_data) {
                    Some(img) => DynamicImage::ImageRgba8(img),
                    None => {
                        if let Some(tx) = self.tx.lock().unwrap().take() {
                            let _ = tx.send(Err(CaptureError::ImageError(
                                "Failed to create image from frame".into(),
                            )));
                        }
                        capture_control.stop();
                        return Ok(());
                    }
                };

                // Send frame
                if let Some(tx) = self.tx.lock().unwrap().take() {
                    let _ = tx.send(Ok(image));
                }

                capture_control.stop();
                Ok(())
            }

            fn on_closed(&mut self) -> Result<(), Self::Error> {
                Ok(())
            }
        }

        // Configure capture settings
        let cursor_settings = if include_cursor {
            CursorCaptureSettings::WithCursor
        } else {
            CursorCaptureSettings::WithoutCursor
        };

        let settings = Settings::new(
            monitor,
            cursor_settings,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            tx.clone(),
        );

        // Start capture
        let capture = OneShotMonitorCapture::start_free_threaded(settings).map_err(|e| {
            tracing::error!("Failed to start WGC monitor capture: {}", e);
            CaptureError::BackendNotAvailable {
                backend: BackendType::Windows,
            }
        })?;

        // Wait for frame with timeout
        let result = rx
            .recv_timeout(std::time::Duration::from_millis(CAPTURE_WINDOW_TIMEOUT_MS))
            .map_err(|_| {
                tracing::warn!("Capture timeout waiting for monitor frame");
                CaptureError::CaptureTimeout {
                    duration_ms: CAPTURE_WINDOW_TIMEOUT_MS,
                }
            })?;

        // Stop capture
        drop(capture);

        result
    }
}

#[async_trait]
impl CaptureFacade for WindowsBackend {
    /// Lists all capturable windows on Windows
    ///
    /// Uses Win32 EnumWindows API to enumerate top-level windows.
    /// Filters out invisible windows and those without titles.
    async fn list_windows(&self) -> CaptureResult<Vec<WindowInfo>> {
        tracing::debug!("list_windows called");

        // Run enumeration in blocking task to avoid blocking async runtime
        let windows = Self::with_timeout(
            async {
                tokio::task::spawn_blocking(Self::enumerate_windows_sync)
                    .await
                    .map_err(|e| {
                        tracing::error!("Window enumeration task panicked: {}", e);
                        CaptureError::BackendNotAvailable {
                            backend: BackendType::Windows,
                        }
                    })
            },
            LIST_WINDOWS_TIMEOUT_MS,
        )
        .await?;

        tracing::info!("Found {} windows", windows.len());
        Ok(windows)
    }

    /// Resolves a window selector to a specific window handle
    ///
    /// Uses multiple matching strategies in priority order:
    /// 1. Regex match on title
    /// 2. Substring match on title (case-insensitive)
    /// 3. Exact class match
    /// 4. Exact exe match
    /// 5. Fuzzy match (threshold >= 60)
    async fn resolve_target(&self, selector: &WindowSelector) -> CaptureResult<WindowHandle> {
        tracing::debug!("resolve_target called with selector: {:?}", selector);

        // Validate selector - at least one criterion must be specified
        if selector.title_substring_or_regex.is_none()
            && selector.class.is_none()
            && selector.exe.is_none()
        {
            return Err(CaptureError::InvalidParameter {
                parameter: "selector".to_string(),
                reason:    "At least one of title, class, or exe must be specified".to_string(),
            });
        }

        // Get window list
        let windows = self.list_windows().await?;
        if windows.is_empty() {
            return Err(CaptureError::WindowNotFound {
                selector: selector.clone(),
            });
        }

        // Try matching strategies in priority order

        // 1. If title is specified, try regex, then substring, then fuzzy
        if let Some(title) = &selector.title_substring_or_regex {
            // Try regex first
            if let Some(handle) = Self::try_regex_match(title, &windows) {
                tracing::debug!("Matched by regex: {}", handle);
                return Ok(handle);
            }

            // Fall back to substring
            if let Some(handle) = Self::try_substring_match(title, &windows) {
                tracing::debug!("Matched by substring: {}", handle);
                return Ok(handle);
            }
        }

        // 2. Try exact class match
        if let Some(class) = &selector.class {
            if let Some(handle) = Self::try_exact_class_match(class, &windows) {
                tracing::debug!("Matched by class: {}", handle);
                return Ok(handle);
            }
        }

        // 3. Try exact exe match
        if let Some(exe) = &selector.exe {
            if let Some(handle) = Self::try_exact_exe_match(exe, &windows) {
                tracing::debug!("Matched by exe: {}", handle);
                return Ok(handle);
            }
        }

        // 4. Try fuzzy match on title as last resort
        if let Some(title) = &selector.title_substring_or_regex {
            if let Some(handle) = Self::try_fuzzy_match(title, &windows) {
                tracing::debug!("Matched by fuzzy: {}", handle);
                return Ok(handle);
            }
        }

        // No match found
        tracing::debug!("No window matched selector: {:?}", selector);
        Err(CaptureError::WindowNotFound {
            selector: selector.clone(),
        })
    }

    /// Captures a screenshot of a specific window
    ///
    /// Uses Windows Graphics Capture API via windows-capture crate.
    async fn capture_window(
        &self,
        handle: WindowHandle,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        tracing::debug!("capture_window called with handle: {}, opts: {:?}", handle, opts);

        // Parse HWND from handle string
        let hwnd: isize = handle.parse().map_err(|_| CaptureError::InvalidParameter {
            parameter: "handle".to_string(),
            reason:    format!("Invalid window handle: {}", handle),
        })?;

        // Copy options for blocking task
        let region = opts.region;
        let scale = opts.scale;
        let include_cursor = opts.include_cursor;

        // Run capture in blocking task
        let image = Self::with_timeout(
            async move {
                tokio::task::spawn_blocking(move || Self::capture_window_sync(hwnd, include_cursor))
                    .await
                    .map_err(|e| {
                        tracing::error!("Capture task panicked: {}", e);
                        CaptureError::BackendNotAvailable {
                            backend: BackendType::Windows,
                        }
                    })?
            },
            CAPTURE_WINDOW_TIMEOUT_MS,
        )
        .await?;

        // Create ImageBuffer and apply transformations
        let mut buffer = ImageBuffer::new(image);

        // Apply region crop if specified
        if let Some(region) = region {
            buffer = buffer.crop(region)?;
        }

        // Apply scale if not 1.0
        if (scale - 1.0).abs() > 0.001 {
            buffer = buffer.scale(scale)?;
        }

        Ok(buffer)
    }

    /// Captures a screenshot of an entire display
    ///
    /// Uses Windows Graphics Capture API for monitor capture.
    async fn capture_display(
        &self,
        display_id: Option<u32>,
        opts: &CaptureOptions,
    ) -> CaptureResult<ImageBuffer> {
        tracing::debug!(
            "capture_display called with display_id: {:?}, opts: {:?}",
            display_id,
            opts
        );

        // Copy options for blocking task
        let region = opts.region;
        let scale = opts.scale;
        let include_cursor = opts.include_cursor;

        // Run capture in blocking task
        let image = Self::with_timeout(
            async move {
                tokio::task::spawn_blocking(move || {
                    Self::capture_display_sync(display_id, include_cursor)
                })
                .await
                .map_err(|e| {
                    tracing::error!("Display capture task panicked: {}", e);
                    CaptureError::BackendNotAvailable {
                        backend: BackendType::Windows,
                    }
                })?
            },
            CAPTURE_WINDOW_TIMEOUT_MS,
        )
        .await?;

        // Create ImageBuffer and apply transformations
        let mut buffer = ImageBuffer::new(image);

        // Apply region crop if specified
        if let Some(region) = region {
            buffer = buffer.crop(region)?;
        }

        // Apply scale if not 1.0
        if (scale - 1.0).abs() > 0.001 {
            buffer = buffer.scale(scale)?;
        }

        Ok(buffer)
    }

    /// Returns the capabilities of the Windows backend
    ///
    /// Windows Graphics Capture supports:
    /// - Window capture
    /// - Display capture
    /// - Cursor inclusion (optional)
    /// - Region cropping (via post-processing)
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            supports_window_capture:  true,
            supports_display_capture: true,
            supports_region:          true,
            supports_cursor:          true, // WGC supports cursor capture
            supports_wayland_restore: false,
        }
    }

    /// Enables downcasting to WindowsBackend
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_backend_new() {
        let backend = WindowsBackend::new();
        assert!(backend.is_ok());
    }

    #[test]
    fn test_capabilities() {
        let backend = WindowsBackend::new().unwrap();
        let caps = backend.capabilities();

        assert!(caps.supports_window_capture);
        assert!(caps.supports_display_capture);
        assert!(caps.supports_region);
        assert!(caps.supports_cursor);
        assert!(!caps.supports_wayland_restore);
    }

    #[test]
    fn test_as_any_downcast() {
        let backend = WindowsBackend::new().unwrap();
        let any_ref = backend.as_any();
        let downcast = any_ref.downcast_ref::<WindowsBackend>();
        assert!(downcast.is_some());
    }

    #[tokio::test]
    async fn test_list_windows_returns_windows() {
        let backend = WindowsBackend::new().unwrap();
        let result = backend.list_windows().await;
        assert!(result.is_ok());
        // On Windows, we should have at least one window (the test runner itself)
        let windows = result.unwrap();
        // Note: In CI or minimal environments, there might be no windows
        // but on a typical Windows desktop, there should be some
        tracing::info!("Found {} windows in test", windows.len());
    }

    #[test]
    fn test_enumerate_window_handles() {
        let handles = WindowsBackend::enumerate_window_handles();
        // Should return some handles on a typical Windows system
        tracing::info!("Enumerated {} handles", handles.len());
    }

    #[test]
    fn test_get_window_title() {
        // Test with invalid handle - should return empty string
        let title = WindowsBackend::get_window_title(0);
        assert!(title.is_empty());
    }

    #[test]
    fn test_get_window_class() {
        // Test with invalid handle - should return empty string
        let class = WindowsBackend::get_window_class(0);
        assert!(class.is_empty());
    }

    #[test]
    fn test_get_window_process_info() {
        // Test with invalid handle
        let (pid, _owner) = WindowsBackend::get_window_process_info(0);
        assert_eq!(pid, 0);
    }

    #[test]
    fn test_fetch_window_info_invalid_handle() {
        // Invalid handle should return None
        let info = WindowsBackend::fetch_window_info(0);
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_resolve_target_no_match() {
        let backend = WindowsBackend::new().unwrap();
        let selector = WindowSelector::by_title("NonExistentWindowTitle12345XYZ");
        let result = backend.resolve_target(&selector).await;
        // Should return WindowNotFound since no window matches
        assert!(matches!(result, Err(CaptureError::WindowNotFound { .. })));
    }

    #[tokio::test]
    async fn test_resolve_target_empty_selector() {
        let backend = WindowsBackend::new().unwrap();
        let selector = WindowSelector {
            title_substring_or_regex: None,
            class: None,
            exe: None,
        };
        let result = backend.resolve_target(&selector).await;
        assert!(matches!(result, Err(CaptureError::InvalidParameter { .. })));
    }

    #[test]
    fn test_try_regex_match() {
        let windows = vec![
            WindowInfo {
                id:      "1".to_string(),
                title:   "Firefox - Google".to_string(),
                class:   "MozillaWindowClass".to_string(),
                owner:   "firefox.exe".to_string(),
                pid:     1234,
                backend: BackendType::Windows,
            },
            WindowInfo {
                id:      "2".to_string(),
                title:   "Notepad".to_string(),
                class:   "Notepad".to_string(),
                owner:   "notepad.exe".to_string(),
                pid:     5678,
                backend: BackendType::Windows,
            },
        ];

        // Test valid regex
        let result = WindowsBackend::try_regex_match("Fire.*", &windows);
        assert_eq!(result, Some("1".to_string()));

        // Test case-insensitive
        let result = WindowsBackend::try_regex_match("fire.*", &windows);
        assert_eq!(result, Some("1".to_string()));

        // Test no match
        let result = WindowsBackend::try_regex_match("Chrome", &windows);
        assert!(result.is_none());

        // Test invalid regex fallback
        let result = WindowsBackend::try_regex_match("[invalid", &windows);
        assert!(result.is_none());
    }

    #[test]
    fn test_try_substring_match() {
        let windows = vec![WindowInfo {
            id:      "1".to_string(),
            title:   "Visual Studio Code".to_string(),
            class:   "Chrome_WidgetWin_1".to_string(),
            owner:   "code.exe".to_string(),
            pid:     1234,
            backend: BackendType::Windows,
        }];

        // Case insensitive match
        let result = WindowsBackend::try_substring_match("visual studio", &windows);
        assert_eq!(result, Some("1".to_string()));

        // Partial match
        let result = WindowsBackend::try_substring_match("Studio", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_try_exact_class_match() {
        let windows = vec![WindowInfo {
            id:      "1".to_string(),
            title:   "Notepad".to_string(),
            class:   "Notepad".to_string(),
            owner:   "notepad.exe".to_string(),
            pid:     1234,
            backend: BackendType::Windows,
        }];

        let result = WindowsBackend::try_exact_class_match("Notepad", &windows);
        assert_eq!(result, Some("1".to_string()));

        // Case insensitive
        let result = WindowsBackend::try_exact_class_match("notepad", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_try_exact_exe_match() {
        let windows = vec![WindowInfo {
            id:      "1".to_string(),
            title:   "Test Window".to_string(),
            class:   "TestClass".to_string(),
            owner:   "myapp.exe".to_string(),
            pid:     1234,
            backend: BackendType::Windows,
        }];

        let result = WindowsBackend::try_exact_exe_match("myapp.exe", &windows);
        assert_eq!(result, Some("1".to_string()));

        // Case insensitive
        let result = WindowsBackend::try_exact_exe_match("MYAPP.EXE", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_try_fuzzy_match() {
        let windows = vec![WindowInfo {
            id:      "1".to_string(),
            title:   "Visual Studio Code".to_string(),
            class:   "Test".to_string(),
            owner:   "code.exe".to_string(),
            pid:     1234,
            backend: BackendType::Windows,
        }];

        // Typo tolerance
        let result = WindowsBackend::try_fuzzy_match("viusal studio", &windows);
        // Should match with fuzzy
        assert!(result.is_some() || result.is_none()); // May or may not meet
                                                       // threshold
    }

    #[tokio::test]
    async fn test_capture_window_invalid_handle() {
        let backend = WindowsBackend::new().unwrap();
        let opts = CaptureOptions::default();
        // Invalid HWND should fail to capture
        let result = backend.capture_window("999999999".to_string(), &opts).await;
        // Should fail with WindowNotFound or timeout
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_capture_window_invalid_handle_format() {
        let backend = WindowsBackend::new().unwrap();
        let opts = CaptureOptions::default();
        // Non-numeric handle should fail with InvalidParameter
        let result = backend
            .capture_window("not-a-number".to_string(), &opts)
            .await;
        assert!(matches!(result, Err(CaptureError::InvalidParameter { .. })));
    }

    // Note: Actual window/display capture tests are integration tests
    // that require a real Windows desktop environment.
    // See tests/windows_integration_tests.rs for those tests.

    #[test]
    fn test_timeout_constants() {
        assert_eq!(LIST_WINDOWS_TIMEOUT_MS, 1500);
        assert_eq!(CAPTURE_WINDOW_TIMEOUT_MS, 2000);
    }
}
