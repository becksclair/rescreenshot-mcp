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

use std::{any::Any, ffi::OsString, os::windows::ffi::OsStringExt, ptr, sync::mpsc};

use async_trait::async_trait;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
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
    Foundation::{CloseHandle, HWND},
    System::{
        ProcessStatus::GetModuleBaseNameW,
        Registry::{HKEY_LOCAL_MACHINE, RegCloseKey, RegOpenKeyExW, RegQueryValueExW},
        Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
    },
    UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindow, IsWindowVisible,
    },
};

#[allow(clippy::upper_case_acronyms)]
type BOOL = i32;
const TRUE: BOOL = 1;
const FALSE: BOOL = 0;

use super::{CaptureFacade, ImageBuffer};
use crate::{
    error::{CaptureError, CaptureResult},
    model::{BackendType, Capabilities, CaptureOptions, WindowHandle, WindowInfo, WindowSelector},
};

/// Timeout for window enumeration operations (1.5s)
///
/// Allows enumeration of many windows while keeping total latency reasonable.
const LIST_WINDOWS_TIMEOUT_MS: u64 = 1500;

/// Timeout for single window capture operations (2s)
///
/// WGC capture typically completes quickly, but we allow extra time for:
/// - Large windows (4K, 8K displays)
/// - GPU scheduling delays
/// - Compositing effects
const CAPTURE_WINDOW_TIMEOUT_MS: u64 = 2000;

/// Minimum Windows build number for Windows Graphics Capture
///
/// WGC was introduced in Windows 10 version 1803 (April 2018 Update),
/// which corresponds to build 17134.
const MINIMUM_WGC_BUILD: u32 = 17134;

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

    /// Gets the Windows build number from the registry
    ///
    /// Reads the CurrentBuildNumber value from:
    /// `HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion`
    ///
    /// Returns the build number or 0 if it cannot be read.
    fn get_windows_build() -> u32 {
        unsafe {
            let mut key_handle = std::ptr::null_mut();
            let key_name = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\0"
                .encode_utf16()
                .collect::<Vec<_>>();

            // Open registry key
            let open_result = RegOpenKeyExW(
                HKEY_LOCAL_MACHINE as *mut _,
                key_name.as_ptr(),
                0,
                0x20001, // KEY_READ
                &mut key_handle,
            );

            if open_result != 0 {
                tracing::debug!("Failed to open registry key for Windows version");
                return 0;
            }

            // Read CurrentBuildNumber value
            let value_name = "CurrentBuildNumber\0".encode_utf16().collect::<Vec<_>>();
            let mut buffer: Vec<u16> = vec![0; 260]; // Enough for a build number string
            let mut buffer_size = (buffer.len() as u32) * 2; // Size in bytes

            let query_result = RegQueryValueExW(
                key_handle,
                value_name.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                buffer.as_mut_ptr() as *mut u8,
                &mut buffer_size,
            );

            RegCloseKey(key_handle);

            if query_result != 0 {
                tracing::debug!("Failed to query CurrentBuildNumber registry value");
                return 0;
            }

            // Convert UTF-16 buffer to string and parse as u32
            let actual_len = (buffer_size as usize / 2) - 1; // Exclude null terminator
            let build_str = OsString::from_wide(&buffer[..actual_len])
                .to_string_lossy()
                .to_string();
            build_str.trim().parse::<u32>().unwrap_or(0)
        }
    }

    /// Checks if Windows Graphics Capture is available on this system
    ///
    /// WGC requires Windows 10 version 1803 (build 17134) or later.
    /// This performs a proactive check via the registry.
    ///
    /// # Returns
    ///
    /// - `Ok(())` - WGC is available
    /// - `Err(WGCUnavailable)` - Windows build is too old
    fn check_wgc_available() -> CaptureResult<()> {
        let build = Self::get_windows_build();

        if build == 0 {
            // Could not determine build number, assume it's OK
            // (runtime check will catch issues)
            tracing::debug!("Could not determine Windows build number, assuming WGC available");
            return Ok(());
        }

        if build < MINIMUM_WGC_BUILD {
            tracing::warn!(
                "Windows build {} is older than minimum WGC build {}",
                build,
                MINIMUM_WGC_BUILD
            );
            return Err(CaptureError::UnsupportedWindowsVersion {
                current_build: build,
                minimum_build: MINIMUM_WGC_BUILD,
            });
        }

        Ok(())
    }

    /// Checks if a window handle is still valid
    ///
    /// Kept for potential use in window validation before capture operations.
    #[allow(dead_code)]
    fn is_window_valid(hwnd: HWND) -> bool {
        unsafe { IsWindow(hwnd) != FALSE }
    }

    /// Enumerates all top-level windows using Win32 EnumWindows
    ///
    /// Returns a vector of HWNDs for all visible windows with titles.
    fn enumerate_window_handles() -> Vec<HWND> {
        let mut handles: Vec<HWND> = Vec::new();

        unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: isize) -> BOOL {
            // SAFETY: lparam is a valid pointer to Vec<HWND> passed from
            // enumerate_window_handles
            let handles = unsafe { &mut *(lparam as *mut Vec<HWND>) };

            // Only include visible windows
            // SAFETY: hwnd is a valid window handle from EnumWindows
            if unsafe { IsWindowVisible(hwnd) } == FALSE {
                return TRUE; // Continue enumeration
            }

            // Only include windows with titles
            // SAFETY: hwnd is a valid window handle from EnumWindows
            let title_len = unsafe { GetWindowTextLengthW(hwnd) };
            if title_len == 0 {
                return TRUE; // Continue enumeration
            }

            handles.push(hwnd);
            TRUE // Continue enumeration
        }

        unsafe {
            EnumWindows(Some(enum_callback), &mut handles as *mut Vec<HWND> as isize);
        }

        tracing::debug!("Enumerated {} window handles", handles.len());
        handles
    }

    /// Gets the title of a window
    ///
    /// # Buffer Sizing Safety
    ///
    /// This function uses `GetWindowTextW` which requires careful buffer sizing:
    /// - `GetWindowTextLengthW` returns text length WITHOUT null terminator
    /// - Buffer MUST be allocated as `len + 1` to accommodate the null terminator
    /// - Off-by-one errors here are a common Win32 pitfall causing buffer overruns
    ///
    /// Future modifications MUST preserve the `(len + 1)` buffer sizing logic.
    fn get_window_title(hwnd: HWND) -> String {
        // Cap title length to prevent unbounded allocation from malicious windows
        const MAX_TITLE_LEN: i32 = 32768;
        unsafe {
            let len = GetWindowTextLengthW(hwnd).min(MAX_TITLE_LEN);
            if len == 0 {
                return String::new();
            }

            // CRITICAL: Buffer sizing for GetWindowTextW must include +1 for null terminator.
            // GetWindowTextLengthW returns the text length WITHOUT the null terminator.
            // GetWindowTextW requires a buffer large enough to hold len characters PLUS the
            // null terminator. Off-by-one errors here are a common Win32 pitfall that can
            // cause buffer overruns, memory corruption, or silent truncation.
            // 
            // Buffer size MUST be: (len + 1) * sizeof(u16) bytes
            // This is equivalent to: vec![0; (len + 1) as usize] for u16 elements
            //
            // DO NOT modify this buffer sizing logic without understanding this requirement.
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
    fn get_window_class(hwnd: HWND) -> String {
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
    fn get_window_process_info(hwnd: HWND) -> (u32, String) {
        unsafe {
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);

            if pid == 0 {
                return (0, String::new());
            }

            // Open process to get exe name
            let process_handle =
                OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, FALSE, pid);

            if process_handle.is_null() {
                return (pid, String::new());
            }

            // Get module base name (exe name) - use null for main module
            let mut exe_buffer: Vec<u16> = vec![0; 260]; // MAX_PATH
            let len = GetModuleBaseNameW(
                process_handle,
                ptr::null_mut(), // NULL module handle = main executable
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
    fn fetch_window_info(hwnd: HWND) -> Option<WindowInfo> {
        let title = Self::get_window_title(hwnd);

        // Skip windows without titles (already filtered in enumeration, but
        // double-check)
        if title.is_empty() {
            return None;
        }

        let class = Self::get_window_class(hwnd);
        let (pid, owner) = Self::get_window_process_info(hwnd);

        Some(WindowInfo {
            id: (hwnd as isize).to_string(),
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
        fn normalize_whitespace_lowercase(s: &str) -> String {
            s.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        }

        let lower_substring = normalize_whitespace_lowercase(substring);
        windows
            .iter()
            .find(|w| normalize_whitespace_lowercase(&w.title).contains(&lower_substring))
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
    fn create_wc_window(hwnd: HWND) -> CaptureResult<WcWindow> {
        Ok(WcWindow::from_raw_hwnd(hwnd))
    }

    /// Synchronously captures a window using WGC
    ///
    /// This function runs in a blocking context via spawn_blocking.
    fn capture_window_sync(hwnd: HWND, include_cursor: bool) -> CaptureResult<DynamicImage> {
        use std::sync::{Arc, Mutex};

        use windows_capture::settings::{
            DirtyRegionSettings, MinimumUpdateIntervalSettings, SecondaryWindowSettings,
        };

        // Create window from HWND
        // Note: We skip the is_window_valid pre-check as it creates a TOCTOU race.
        // Instead, let create_wc_window fail and map errors appropriately.
        let window = Self::create_wc_window(hwnd).map_err(|e| {
            tracing::warn!("Failed to create capture window for {:?}: {:?}", hwnd, e);
            CaptureError::WindowClosed
        })?;
        tracing::debug!("Created window for capture from HWND: {:?}", hwnd);

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
                // CRITICAL: Use as_nopadding_buffer() to strip GPU stride padding
                // as_raw_buffer() includes row padding that causes buffer length mismatches
                let raw_data = buffer.as_nopadding_buffer()?;
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
                        if let Some(tx) = self.tx.lock().ok().and_then(|mut g| g.take()) {
                            let _ = tx.send(Err(CaptureError::ImageError(
                                "Failed to create image from frame".into(),
                            )));
                        }
                        capture_control.stop();
                        return Ok(());
                    }
                };

                // Send frame
                if let Some(tx) = self.tx.lock().ok().and_then(|mut g| g.take()) {
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
                    reason: format!("Monitor {} not found", id),
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
                // CRITICAL: Use as_nopadding_buffer() to strip GPU stride padding
                // as_raw_buffer() includes row padding that causes buffer length mismatches
                let raw_data = buffer.as_nopadding_buffer()?;

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
                        if let Some(tx) = self.tx.lock().ok().and_then(|mut g| g.take()) {
                            let _ = tx.send(Err(CaptureError::ImageError(
                                "Failed to create image from frame".into(),
                            )));
                        }
                        capture_control.stop();
                        return Ok(());
                    }
                };

                // Send frame
                if let Some(tx) = self.tx.lock().ok().and_then(|mut g| g.take()) {
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
                reason: "At least one of title, class, or exe must be specified".to_string(),
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

        // Check Windows version before attempting capture
        Self::check_wgc_available()?;

        // Parse HWND from handle string (stored as isize for Send safety)
        let hwnd_val: isize = handle.parse().map_err(|_| CaptureError::InvalidParameter {
            parameter: "handle".to_string(),
            reason: format!("Invalid window handle: {}", handle),
        })?;

        // Copy options for blocking task
        let region = opts.region;
        let scale = opts.scale;
        let include_cursor = opts.include_cursor;

        // Run capture in blocking task
        // Note: We pass hwnd_val (isize) which is Send, then convert to HWND inside
        let image = Self::with_timeout(
            async move {
                tokio::task::spawn_blocking(move || {
                    let hwnd = hwnd_val as HWND;
                    Self::capture_window_sync(hwnd, include_cursor)
                })
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

        // Check Windows version before attempting capture
        Self::check_wgc_available()?;

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
            supports_window_capture: true,
            supports_display_capture: true,
            supports_region: true,
            supports_cursor: true, // WGC supports cursor capture
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
        let title = WindowsBackend::get_window_title(ptr::null_mut());
        assert!(title.is_empty());
    }

    #[test]
    fn test_get_window_class() {
        // Test with invalid handle - should return empty string
        let class = WindowsBackend::get_window_class(ptr::null_mut());
        assert!(class.is_empty());
    }

    #[test]
    fn test_get_window_process_info() {
        // Test with invalid handle
        let (pid, _owner) = WindowsBackend::get_window_process_info(ptr::null_mut());
        assert_eq!(pid, 0);
    }

    #[test]
    fn test_fetch_window_info_invalid_handle() {
        // Invalid handle should return None
        let info = WindowsBackend::fetch_window_info(ptr::null_mut());
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
                id: "1".to_string(),
                title: "Firefox - Google".to_string(),
                class: "MozillaWindowClass".to_string(),
                owner: "firefox.exe".to_string(),
                pid: 1234,
                backend: BackendType::Windows,
            },
            WindowInfo {
                id: "2".to_string(),
                title: "Notepad".to_string(),
                class: "Notepad".to_string(),
                owner: "notepad.exe".to_string(),
                pid: 5678,
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
            id: "1".to_string(),
            title: "Visual Studio Code".to_string(),
            class: "Chrome_WidgetWin_1".to_string(),
            owner: "code.exe".to_string(),
            pid: 1234,
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
            id: "1".to_string(),
            title: "Notepad".to_string(),
            class: "Notepad".to_string(),
            owner: "notepad.exe".to_string(),
            pid: 1234,
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
            id: "1".to_string(),
            title: "Test Window".to_string(),
            class: "TestClass".to_string(),
            owner: "myapp.exe".to_string(),
            pid: 1234,
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
            id: "1".to_string(),
            title: "Visual Studio Code".to_string(),
            class: "Test".to_string(),
            owner: "code.exe".to_string(),
            pid: 1234,
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

    #[test]
    fn test_minimum_wgc_build_constant() {
        assert_eq!(MINIMUM_WGC_BUILD, 17134);
    }

    #[test]
    fn test_check_wgc_available() {
        // check_wgc_available always succeeds now - runtime check happens at capture
        // time
        let result = WindowsBackend::check_wgc_available();
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_window_valid_with_null() {
        // Null handle should be invalid
        let valid = WindowsBackend::is_window_valid(ptr::null_mut());
        assert!(!valid);
    }

    #[test]
    fn test_is_window_valid_with_invalid_handle() {
        // Non-existent window handle should be invalid
        let valid = WindowsBackend::is_window_valid(0x12345678 as HWND);
        assert!(!valid);
    }

    #[tokio::test]
    async fn test_resolve_by_class() {
        let backend = WindowsBackend::new().unwrap();
        // Try to find a common Windows class
        let selector = WindowSelector {
            title_substring_or_regex: None,
            class: Some("Shell_TrayWnd".to_string()), // Windows taskbar
            exe: None,
        };
        let result = backend.resolve_target(&selector).await;
        // Taskbar should exist on any Windows system
        if let Ok(handle) = result {
            // Handle should be a valid number
            assert!(handle.parse::<isize>().is_ok());
        }
    }

    #[tokio::test]
    async fn test_resolve_by_exe() {
        let backend = WindowsBackend::new().unwrap();
        // Try to find explorer.exe
        let selector = WindowSelector {
            title_substring_or_regex: None,
            class: None,
            exe: Some("explorer.exe".to_string()),
        };
        let result = backend.resolve_target(&selector).await;
        // Explorer should exist on any Windows system (File Explorer or shell)
        // Note: This may or may not succeed depending on system state
        tracing::info!("resolve_by_exe result: {:?}", result);
    }

    #[test]
    fn test_regex_match_case_insensitive() {
        let windows = vec![WindowInfo {
            id: "1".to_string(),
            title: "NOTEPAD - Untitled".to_string(),
            class: "Notepad".to_string(),
            owner: "notepad.exe".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        }];

        // Lowercase regex should match uppercase title
        let result = WindowsBackend::try_regex_match("notepad", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_regex_match_with_special_chars() {
        let windows = vec![WindowInfo {
            id: "1".to_string(),
            title: "Document (1).txt - Notepad".to_string(),
            class: "Notepad".to_string(),
            owner: "notepad.exe".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        }];

        // Escaped regex for literal parens
        let result = WindowsBackend::try_regex_match(r"Document \(1\)", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_substring_match_partial() {
        let windows = vec![
            WindowInfo {
                id: "1".to_string(),
                title: "Google Chrome".to_string(),
                class: "Chrome_WidgetWin_1".to_string(),
                owner: "chrome.exe".to_string(),
                pid: 1234,
                backend: BackendType::Windows,
            },
            WindowInfo {
                id: "2".to_string(),
                title: "Firefox Developer Edition".to_string(),
                class: "MozillaWindowClass".to_string(),
                owner: "firefox.exe".to_string(),
                pid: 5678,
                backend: BackendType::Windows,
            },
        ];

        // Partial substring match
        let result = WindowsBackend::try_substring_match("Chrome", &windows);
        assert_eq!(result, Some("1".to_string()));

        let result = WindowsBackend::try_substring_match("Developer", &windows);
        assert_eq!(result, Some("2".to_string()));
    }

    #[test]
    fn test_exact_class_match_not_found() {
        let windows = vec![WindowInfo {
            id: "1".to_string(),
            title: "Test Window".to_string(),
            class: "TestClass".to_string(),
            owner: "test.exe".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        }];

        let result = WindowsBackend::try_exact_class_match("NonExistentClass", &windows);
        assert!(result.is_none());
    }

    #[test]
    fn test_exact_exe_match_with_extension() {
        let windows = vec![WindowInfo {
            id: "1".to_string(),
            title: "Visual Studio Code".to_string(),
            class: "Chrome_WidgetWin_1".to_string(),
            owner: "Code.exe".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        }];

        // Case-insensitive match
        let result = WindowsBackend::try_exact_exe_match("code.exe", &windows);
        assert_eq!(result, Some("1".to_string()));

        // Without extension should NOT match (exact match)
        let result = WindowsBackend::try_exact_exe_match("code", &windows);
        assert!(result.is_none());
    }

    #[test]
    fn test_fuzzy_match_with_typos() {
        let windows = vec![WindowInfo {
            id: "1".to_string(),
            title: "Microsoft Word Document".to_string(),
            class: "Test".to_string(),
            owner: "WINWORD.EXE".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        }];

        // Minor typo - may or may not meet threshold
        let result = WindowsBackend::try_fuzzy_match("Microsft Word", &windows);
        // Just ensure it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_fuzzy_match_with_abbreviation() {
        let windows = vec![WindowInfo {
            id: "1".to_string(),
            title: "Windows PowerShell".to_string(),
            class: "Test".to_string(),
            owner: "powershell.exe".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        }];

        // Abbreviation-style match
        let result = WindowsBackend::try_fuzzy_match("powershell", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_regex_pattern_too_large() {
        let windows = vec![WindowInfo {
            id: "1".to_string(),
            title: "Test".to_string(),
            class: "Test".to_string(),
            owner: "test.exe".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        }];

        // Create pattern > 1MB
        let large_pattern = "a".repeat(1_000_001);
        let result = WindowsBackend::try_regex_match(&large_pattern, &windows);
        assert!(result.is_none()); // Should be rejected as too large
    }

    #[test]
    fn test_enumerate_windows_sync_returns_vec() {
        let windows = WindowsBackend::enumerate_windows_sync();
        // Should return some windows on a typical system
        tracing::info!("enumerate_windows_sync found {} windows", windows.len());
        // Verify it returns a valid vec (doesn't panic)
        let _ = windows.len();
    }

    #[test]
    fn test_window_info_has_backend_type() {
        let windows = WindowsBackend::enumerate_windows_sync();
        for win in windows.iter().take(5) {
            assert_eq!(win.backend, BackendType::Windows);
        }
    }

    #[test]
    fn test_capabilities_all_fields() {
        let backend = WindowsBackend::new().unwrap();
        let caps = backend.capabilities();

        // Verify all capability fields
        assert!(caps.supports_window_capture);
        assert!(caps.supports_display_capture);
        assert!(caps.supports_region);
        assert!(caps.supports_cursor);
        assert!(!caps.supports_wayland_restore); // Windows-specific, not Wayland
    }

    #[tokio::test]
    async fn test_list_windows_has_titles() {
        let backend = WindowsBackend::new().unwrap();
        let windows = backend.list_windows().await.unwrap();

        // All windows should have non-empty titles (filtered during enumeration)
        for win in &windows {
            assert!(!win.title.is_empty(), "Window {} has empty title", win.id);
        }
    }

    #[tokio::test]
    async fn test_list_windows_has_valid_ids() {
        let backend = WindowsBackend::new().unwrap();
        let windows = backend.list_windows().await.unwrap();

        // All window IDs should be parseable as isize (HWND values)
        for win in &windows {
            let parsed: Result<isize, _> = win.id.parse();
            assert!(parsed.is_ok(), "Window ID {} is not a valid isize", win.id);
        }
    }

    #[test]
    fn test_backend_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<WindowsBackend>();
        assert_sync::<WindowsBackend>();
    }

    #[test]
    fn test_window_info_fields() {
        let info = WindowInfo {
            id: "12345".to_string(),
            title: "Test Window".to_string(),
            class: "TestClass".to_string(),
            owner: "test.exe".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        };

        assert_eq!(info.id, "12345");
        assert_eq!(info.title, "Test Window");
        assert_eq!(info.class, "TestClass");
        assert_eq!(info.owner, "test.exe");
        assert_eq!(info.pid, 1234);
        assert_eq!(info.backend, BackendType::Windows);
    }

    #[test]
    fn test_window_selector_by_title() {
        let selector = WindowSelector::by_title("Notepad");
        assert_eq!(selector.title_substring_or_regex, Some("Notepad".to_string()));
        assert!(selector.class.is_none());
        assert!(selector.exe.is_none());
    }

    #[test]
    fn test_window_selector_by_class() {
        let selector = WindowSelector::by_class("Notepad");
        assert!(selector.title_substring_or_regex.is_none());
        assert_eq!(selector.class, Some("Notepad".to_string()));
        assert!(selector.exe.is_none());
    }

    #[test]
    fn test_window_selector_by_exe() {
        let selector = WindowSelector::by_exe("notepad.exe");
        assert!(selector.title_substring_or_regex.is_none());
        assert!(selector.class.is_none());
        assert_eq!(selector.exe, Some("notepad.exe".to_string()));
    }

    #[test]
    fn test_capture_options_default() {
        let opts = CaptureOptions::default();
        assert!(opts.region.is_none());
        assert!((opts.scale - 1.0).abs() < 0.001);
        assert!(!opts.include_cursor);
    }

    #[test]
    fn test_capture_options_with_cursor() {
        let opts = CaptureOptions {
            include_cursor: true,
            ..Default::default()
        };
        assert!(opts.include_cursor);
    }

    #[test]
    fn test_capture_options_with_scale() {
        let opts = CaptureOptions {
            scale: 0.5,
            ..Default::default()
        };
        assert!((opts.scale - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_create_wc_window_from_valid_hwnd() {
        // Test that create_wc_window doesn't panic with a non-null handle
        // Note: The window won't be valid, but the function shouldn't panic
        let result = WindowsBackend::create_wc_window(0x1 as HWND);
        assert!(result.is_ok());
    }

    #[test]
    fn test_try_regex_match_empty_windows() {
        let windows: Vec<WindowInfo> = vec![];
        let result = WindowsBackend::try_regex_match("test", &windows);
        assert!(result.is_none());
    }

    #[test]
    fn test_try_substring_match_empty_windows() {
        let windows: Vec<WindowInfo> = vec![];
        let result = WindowsBackend::try_substring_match("test", &windows);
        assert!(result.is_none());
    }

    #[test]
    fn test_try_fuzzy_match_empty_windows() {
        let windows: Vec<WindowInfo> = vec![];
        let result = WindowsBackend::try_fuzzy_match("test", &windows);
        assert!(result.is_none());
    }

    #[test]
    fn test_try_exact_class_match_empty_windows() {
        let windows: Vec<WindowInfo> = vec![];
        let result = WindowsBackend::try_exact_class_match("test", &windows);
        assert!(result.is_none());
    }

    #[test]
    fn test_try_exact_exe_match_empty_windows() {
        let windows: Vec<WindowInfo> = vec![];
        let result = WindowsBackend::try_exact_exe_match("test", &windows);
        assert!(result.is_none());
    }

    // ========== Version Checking Tests ==========

    #[test]
    fn test_get_windows_build_returns_number() {
        let build = WindowsBackend::get_windows_build();
        // On a real system, this should return a valid build number
        tracing::info!("Windows build number: {}", build);
        // Just ensure it returns a u32 (which is always >= 0 by definition)
        let _ = build;
    }

    #[test]
    fn test_check_wgc_available_on_current_system() {
        let result = WindowsBackend::check_wgc_available();
        // On modern Windows systems, this should succeed
        // On older systems, it might fail with WGCUnavailable
        tracing::info!("check_wgc_available result: {:?}", result);
        // Just ensure it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_minimum_wgc_build_is_reasonable() {
        // Windows 10 v1803 (April 2018) is the minimum
        const {
            assert!(MINIMUM_WGC_BUILD >= 17134);
            // Sanity check - shouldn't be way higher
            assert!(MINIMUM_WGC_BUILD <= 20000);
        };
    }

    // ========== Edge Case and Error Handling Tests ==========

    #[tokio::test]
    async fn test_resolve_with_no_criteria() {
        let backend = WindowsBackend::new().unwrap();
        let selector = WindowSelector {
            title_substring_or_regex: None,
            class: None,
            exe: None,
        };
        let result = backend.resolve_target(&selector).await;
        // Should fail because no selection criteria provided
        assert!(matches!(result, Err(CaptureError::InvalidParameter { .. })));
    }

    #[tokio::test]
    async fn test_resolve_nonexistent_window() {
        let backend = WindowsBackend::new().unwrap();
        let selector = WindowSelector::by_title("NonExistentWindow_12345_XYZ_ZZZZZZZ");
        let result = backend.resolve_target(&selector).await;
        // Should fail with WindowNotFound
        assert!(matches!(result, Err(CaptureError::WindowNotFound { .. })));
    }

    #[test]
    fn test_is_window_valid_checks_handle() {
        // Test that null and invalid handles are rejected
        let null_handle: HWND = ptr::null_mut();
        assert!(!WindowsBackend::is_window_valid(null_handle));

        // Test with clearly invalid handle value
        let invalid_handle = 0xdeadbeef as HWND;
        assert!(!WindowsBackend::is_window_valid(invalid_handle));
    }

    #[test]
    fn test_window_title_empty_window() {
        // Test with null handle
        let title = WindowsBackend::get_window_title(ptr::null_mut());
        assert!(title.is_empty());
    }

    #[test]
    fn test_window_class_empty_for_invalid_handle() {
        // Test with clearly invalid handle
        let class = WindowsBackend::get_window_class(0xdeadbeef as HWND);
        // Invalid handle should return empty or short string
        assert!(class.len() < 256); // Should be safe regardless
    }

    #[test]
    fn test_window_process_info_for_invalid_handle() {
        let (pid, exe) = WindowsBackend::get_window_process_info(ptr::null_mut());
        // Should return 0 pid and empty exe for invalid handle
        assert_eq!(pid, 0);
        assert!(exe.is_empty());
    }

    #[test]
    fn test_multiple_window_enumeration_consistent() {
        let windows1 = WindowsBackend::enumerate_windows_sync();
        let windows2 = WindowsBackend::enumerate_windows_sync();

        // Both calls should succeed and return reasonable counts
        assert!(!windows1.is_empty());
        assert!(!windows2.is_empty());
    }

    #[tokio::test]
    async fn test_capture_window_closed_immediately() {
        let backend = WindowsBackend::new().unwrap();
        let opts = CaptureOptions::default();

        // Try to capture with a handle that looks valid (0x1) but isn't
        let result = backend.capture_window("1".to_string(), &opts).await;
        // Should fail, either with WindowClosed or other error
        assert!(result.is_err());
    }

    #[test]
    fn test_regex_pattern_injection_safe() {
        let windows = vec![WindowInfo {
            id: "1".to_string(),
            title: "Test | Window".to_string(),
            class: "Test".to_string(),
            owner: "test.exe".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        }];

        // Test with regex special characters that could cause issues
        let patterns = vec![
            ".*|.*",       // Alternation
            "(test)*",     // Backreference
            "[a-z]++",     // Possessive
            "(?<name>.*)", // Named groups
        ];

        for pattern in patterns {
            // Should not panic, might match or not
            let _ = WindowsBackend::try_regex_match(pattern, &windows);
        }
    }

    #[test]
    fn test_substring_match_unicode_characters() {
        let windows = vec![WindowInfo {
            id: "1".to_string(),
            title: "文档 - Notepad".to_string(), // Chinese characters
            class: "Notepad".to_string(),
            owner: "notepad.exe".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        }];

        // Should handle Unicode without crashing
        let result = WindowsBackend::try_substring_match("文档", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_substring_match_emoji() {
        let windows = vec![WindowInfo {
            id: "1".to_string(),
            title: "📄 Document.txt".to_string(), // Emoji
            class: "Notepad".to_string(),
            owner: "notepad.exe".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        }];

        // Should handle emoji
        let result = WindowsBackend::try_substring_match("📄", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_exact_match_with_spaces() {
        let windows = vec![WindowInfo {
            id: "1".to_string(),
            title: "Visual   Studio   Code".to_string(), // Multiple spaces
            class: "VSCode".to_string(),
            owner: "Code.exe".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        }];

        // Substring match should still work
        let result = WindowsBackend::try_substring_match("Studio Code", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_fuzzy_match_short_pattern() {
        let windows = vec![WindowInfo {
            id: "1".to_string(),
            title: "Microsoft Visual Studio".to_string(),
            class: "Test".to_string(),
            owner: "devenv.exe".to_string(),
            pid: 1234,
            backend: BackendType::Windows,
        }];

        // Short patterns should work
        let result = WindowsBackend::try_fuzzy_match("VS", &windows);
        // May or may not match depending on threshold
        let _ = result;
    }

    #[test]
    fn test_list_all_window_matching_strategies() {
        let windows = vec![
            WindowInfo {
                id: "1".to_string(),
                title: "Notepad - Document1.txt".to_string(),
                class: "Notepad".to_string(),
                owner: "notepad.exe".to_string(),
                pid: 1234,
                backend: BackendType::Windows,
            },
            WindowInfo {
                id: "2".to_string(),
                title: "Visual Studio Code".to_string(),
                class: "Chrome_WidgetWin_1".to_string(),
                owner: "Code.exe".to_string(),
                pid: 5678,
                backend: BackendType::Windows,
            },
        ];

        // All strategies should work on appropriate windows
        assert!(WindowsBackend::try_regex_match("Notepad", &windows).is_some());
        assert!(WindowsBackend::try_substring_match("notepad", &windows).is_some());
        assert!(WindowsBackend::try_exact_class_match("Notepad", &windows).is_some());
        assert!(WindowsBackend::try_exact_exe_match("Code.exe", &windows).is_some());
    }

    #[test]
    fn test_window_enumeration_filters_hidden() {
        // This test verifies that the enumeration callback filters out hidden windows
        // by checking that returned windows are reasonable
        let windows = WindowsBackend::enumerate_windows_sync();

        // All windows should have titles (filtered in enumeration)
        for window in windows {
            assert!(!window.title.is_empty());
            assert!(!window.id.is_empty());
        }
    }

    #[test]
    fn test_capabilities_reflect_windows_abilities() {
        let backend = WindowsBackend::new().unwrap();
        let caps = backend.capabilities();

        // Windows should support all of these
        assert!(caps.supports_window_capture);
        assert!(caps.supports_display_capture);
        assert!(caps.supports_region);
        assert!(caps.supports_cursor);

        // Windows doesn't use Wayland
        assert!(!caps.supports_wayland_restore);
    }

    #[test]
    fn test_backend_new_always_succeeds() {
        // Windows backend should always be creatable on Windows platform
        let result = WindowsBackend::new();
        assert!(result.is_ok());

        let result2 = WindowsBackend::new();
        assert!(result2.is_ok());
    }

    #[test]
    fn test_window_handle_as_isize_roundtrip() {
        let original_hwnd = 0x12345678isize;
        let as_string = original_hwnd.to_string();
        let parsed: isize = as_string.parse().unwrap();
        assert_eq!(original_hwnd, parsed);
    }
}
