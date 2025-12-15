//! Centralized timeout and configuration constants for screenshot capture.
//!
//! This module consolidates timeout values and other constants used across
//! platform-specific backends. Centralizing these values ensures consistency
//! and makes it easier to understand and tune performance characteristics.
//!
//! # Runtime Configuration
//!
//! All timeout values can be overridden at runtime via environment variables:
//!
//! | Environment Variable | Default | Description |
//! |---------------------|---------|-------------|
//! | `SCREENSHOT_LIST_WINDOWS_TIMEOUT_MS` | 1500 | Window enumeration timeout |
//! | `SCREENSHOT_X11_CAPTURE_TIMEOUT_MS` | 2000 | X11 capture timeout |
//! | `SCREENSHOT_WINDOWS_CAPTURE_TIMEOUT_MS` | 5000 | Windows capture timeout |
//! | `SCREENSHOT_WAYLAND_PORTAL_TIMEOUT_SECS` | 30 | Wayland portal timeout |
//! | `SCREENSHOT_PIPEWIRE_FRAME_TIMEOUT_SECS` | 5 | PipeWire frame timeout |
//!
//! # Timeout Philosophy
//!
//! Timeouts are calibrated based on several factors:
//!
//! 1. **User expectations**: Screenshot tools should feel responsive
//! 2. **Platform characteristics**: Some operations are inherently slower
//! 3. **Failure detection**: Timeouts should be short enough to detect hangs
//!    but long enough to avoid false positives on slow systems
//!
//! # Platform-Specific Considerations
//!
//! ## Windows
//! - Windows Graphics Capture API can be slow on first invocation (~1-2s)
//! - HDR/SDR tone mapping adds overhead
//! - DWM (Desktop Window Manager) composition adds latency
//! - Use 5s capture timeout to accommodate these factors
//!
//! ## X11
//! - XShm/XGetImage operations are generally fast
//! - xcap uses optimized capture paths
//! - Use 2s capture timeout (per M3 spec requirements)
//!
//! ## Wayland
//! - Portal operations require user interaction (permission dialogs)
//! - PipeWire stream setup has inherent latency
//! - Use 30s for portal operations (user may need time to respond)
//! - Use 5s for PipeWire frames (should be fast once stream is established)

/// Timeout for listing/enumerating windows across all platforms.
///
/// Window enumeration is a fast operation that queries the window manager
/// for the current window list. 1.5 seconds is generous and should only
/// trigger if the window system is severely unresponsive.
///
/// Used by: X11, Windows
pub const LIST_WINDOWS_TIMEOUT_MS: u64 = 1500;

/// Default capture timeout for X11 backend operations.
///
/// X11 screen capture via xcap/XShm is typically fast (<500ms). The 2-second
/// timeout is set per M3 milestone requirements and provides headroom for:
/// - Slow X servers (especially over network/SSH forwarding)
/// - High-resolution displays (more data to transfer)
/// - Compositors with heavy effects
///
/// Used by: X11 backend
pub const X11_CAPTURE_TIMEOUT_MS: u64 = 2000;

/// Default capture timeout for Windows backend operations.
///
/// Windows Graphics Capture API requires additional time because:
/// - First capture may trigger DLL loading and GPU resource allocation
/// - HDR to SDR tone mapping (on HDR displays)
/// - DWM composition and window texture readback
/// - Frame callback delivery can be delayed under GPU load
///
/// The 5-second timeout accommodates these factors while still detecting
/// genuine hangs (e.g., graphics driver crashes).
///
/// Used by: Windows backend
pub const WINDOWS_CAPTURE_TIMEOUT_MS: u64 = 5000;

/// Timeout for Wayland portal operations (permission dialogs, session setup).
///
/// Portal operations involve D-Bus communication with the desktop portal
/// service and often require user interaction (e.g., selecting which
/// screen/window to share). 30 seconds provides ample time for:
/// - User to read and respond to permission dialogs
/// - Slow D-Bus roundtrips on loaded systems
/// - Portal service initialization on first use
///
/// Used by: Wayland backend (create_session, select_sources, start)
pub const WAYLAND_PORTAL_TIMEOUT_SECS: u64 = 30;

/// Timeout for Wayland portal operations in milliseconds.
///
/// Same as [`WAYLAND_PORTAL_TIMEOUT_SECS`] but in milliseconds for
/// consistency with other timeout constants.
pub const WAYLAND_PORTAL_TIMEOUT_MS: u64 = WAYLAND_PORTAL_TIMEOUT_SECS * 1000;

/// Timeout for PipeWire frame capture after stream is established.
///
/// Once a PipeWire stream is running, frames should arrive quickly (typically
/// within one compositor refresh cycle, ~16ms at 60Hz). A 5-second timeout
/// catches cases where:
/// - The compositor stops producing frames (window minimized, occluded)
/// - PipeWire buffer stalls
/// - GPU/CPU contention causes frame delivery delays
///
/// Used by: Wayland backend (capture_from_pipewire_stream)
pub const PIPEWIRE_FRAME_TIMEOUT_SECS: u64 = 5;

/// Timeout for PipeWire frame capture in milliseconds.
///
/// Same as [`PIPEWIRE_FRAME_TIMEOUT_SECS`] but in milliseconds for
/// consistency with other timeout constants.
pub const PIPEWIRE_FRAME_TIMEOUT_MS: u64 = PIPEWIRE_FRAME_TIMEOUT_SECS * 1000;

/// PipeWire main loop iteration timeout.
///
/// How long to wait in each PipeWire main loop iteration before checking
/// if a frame has arrived or the overall timeout has elapsed.
///
/// Shorter = more responsive timeout detection, more CPU usage
/// Longer = less responsive, less CPU usage
///
/// 10ms is a good balance (100 iterations/second worst case).
pub const PIPEWIRE_LOOP_ITERATION_MS: u64 = 10;

// =============================================================================
// Environment Variable Overrides
// =============================================================================

/// Helper to get a timeout from environment variable or fall back to default.
fn get_timeout_from_env(env_var: &str, default: u64) -> u64 {
    std::env::var(env_var)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Get the window listing timeout, checking environment variable override.
///
/// Override with: `SCREENSHOT_LIST_WINDOWS_TIMEOUT_MS`
///
/// # Example
///
/// ```bash
/// # Set timeout to 3 seconds for slow systems
/// export SCREENSHOT_LIST_WINDOWS_TIMEOUT_MS=3000
/// ```
pub fn list_windows_timeout_ms() -> u64 {
    get_timeout_from_env("SCREENSHOT_LIST_WINDOWS_TIMEOUT_MS", LIST_WINDOWS_TIMEOUT_MS)
}

/// Get the X11 capture timeout, checking environment variable override.
///
/// Override with: `SCREENSHOT_X11_CAPTURE_TIMEOUT_MS`
///
/// # Example
///
/// ```bash
/// # Set timeout to 5 seconds for remote X11 connections
/// export SCREENSHOT_X11_CAPTURE_TIMEOUT_MS=5000
/// ```
pub fn x11_capture_timeout_ms() -> u64 {
    get_timeout_from_env("SCREENSHOT_X11_CAPTURE_TIMEOUT_MS", X11_CAPTURE_TIMEOUT_MS)
}

/// Get the Windows capture timeout, checking environment variable override.
///
/// Override with: `SCREENSHOT_WINDOWS_CAPTURE_TIMEOUT_MS`
///
/// # Example
///
/// ```bash
/// # Set timeout to 10 seconds for slow GPU or HDR displays
/// export SCREENSHOT_WINDOWS_CAPTURE_TIMEOUT_MS=10000
/// ```
pub fn windows_capture_timeout_ms() -> u64 {
    get_timeout_from_env("SCREENSHOT_WINDOWS_CAPTURE_TIMEOUT_MS", WINDOWS_CAPTURE_TIMEOUT_MS)
}

/// Get the Wayland portal timeout in seconds, checking environment variable.
///
/// Override with: `SCREENSHOT_WAYLAND_PORTAL_TIMEOUT_SECS`
///
/// # Example
///
/// ```bash
/// # Set timeout to 60 seconds for users who need more time
/// export SCREENSHOT_WAYLAND_PORTAL_TIMEOUT_SECS=60
/// ```
pub fn wayland_portal_timeout_secs() -> u64 {
    get_timeout_from_env("SCREENSHOT_WAYLAND_PORTAL_TIMEOUT_SECS", WAYLAND_PORTAL_TIMEOUT_SECS)
}

/// Get the Wayland portal timeout in milliseconds.
///
/// This is a convenience wrapper around [`wayland_portal_timeout_secs`].
pub fn wayland_portal_timeout_ms() -> u64 {
    wayland_portal_timeout_secs() * 1000
}

/// Get the PipeWire frame timeout in seconds, checking environment variable.
///
/// Override with: `SCREENSHOT_PIPEWIRE_FRAME_TIMEOUT_SECS`
///
/// # Example
///
/// ```bash
/// # Set timeout to 10 seconds for slow compositor setups
/// export SCREENSHOT_PIPEWIRE_FRAME_TIMEOUT_SECS=10
/// ```
pub fn pipewire_frame_timeout_secs() -> u64 {
    get_timeout_from_env("SCREENSHOT_PIPEWIRE_FRAME_TIMEOUT_SECS", PIPEWIRE_FRAME_TIMEOUT_SECS)
}

/// Get the PipeWire frame timeout in milliseconds.
///
/// This is a convenience wrapper around [`pipewire_frame_timeout_secs`].
pub fn pipewire_frame_timeout_ms() -> u64 {
    pipewire_frame_timeout_secs() * 1000
}

#[cfg(test)]
#[allow(clippy::assertions_on_constants)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_sanity() {
        // All timeouts should be positive
        assert!(LIST_WINDOWS_TIMEOUT_MS > 0);
        assert!(X11_CAPTURE_TIMEOUT_MS > 0);
        assert!(WINDOWS_CAPTURE_TIMEOUT_MS > 0);
        assert!(WAYLAND_PORTAL_TIMEOUT_SECS > 0);
        assert!(PIPEWIRE_FRAME_TIMEOUT_SECS > 0);

        // Window listing should be faster than capture
        assert!(LIST_WINDOWS_TIMEOUT_MS < X11_CAPTURE_TIMEOUT_MS);
        assert!(LIST_WINDOWS_TIMEOUT_MS < WINDOWS_CAPTURE_TIMEOUT_MS);

        // Portal timeout should be longest (user interaction)
        assert!(WAYLAND_PORTAL_TIMEOUT_MS > WINDOWS_CAPTURE_TIMEOUT_MS);
        assert!(WAYLAND_PORTAL_TIMEOUT_MS > X11_CAPTURE_TIMEOUT_MS);
    }

    #[test]
    fn test_millisecond_conversions() {
        assert_eq!(WAYLAND_PORTAL_TIMEOUT_MS, 30_000);
        assert_eq!(PIPEWIRE_FRAME_TIMEOUT_MS, 5_000);
    }

    #[test]
    fn test_reasonable_bounds() {
        // No timeout should exceed 5 minutes (clearly a bug)
        const MAX_REASONABLE_TIMEOUT_MS: u64 = 5 * 60 * 1000;

        assert!(LIST_WINDOWS_TIMEOUT_MS < MAX_REASONABLE_TIMEOUT_MS);
        assert!(X11_CAPTURE_TIMEOUT_MS < MAX_REASONABLE_TIMEOUT_MS);
        assert!(WINDOWS_CAPTURE_TIMEOUT_MS < MAX_REASONABLE_TIMEOUT_MS);
        assert!(WAYLAND_PORTAL_TIMEOUT_MS < MAX_REASONABLE_TIMEOUT_MS);
        assert!(PIPEWIRE_FRAME_TIMEOUT_MS < MAX_REASONABLE_TIMEOUT_MS);

        // PipeWire loop iteration should be < 1 second
        assert!(PIPEWIRE_LOOP_ITERATION_MS < 1000);
    }

    #[test]
    fn test_env_override_defaults() {
        // Without env vars set, functions should return defaults
        // Note: These tests may fail if env vars are set in the test environment
        // In production, we use temp-env for isolation
        assert_eq!(list_windows_timeout_ms(), LIST_WINDOWS_TIMEOUT_MS);
        assert_eq!(x11_capture_timeout_ms(), X11_CAPTURE_TIMEOUT_MS);
        assert_eq!(windows_capture_timeout_ms(), WINDOWS_CAPTURE_TIMEOUT_MS);
        assert_eq!(wayland_portal_timeout_secs(), WAYLAND_PORTAL_TIMEOUT_SECS);
        assert_eq!(pipewire_frame_timeout_secs(), PIPEWIRE_FRAME_TIMEOUT_SECS);
    }

    #[test]
    fn test_env_override_ms_wrappers() {
        // Test millisecond wrapper functions
        assert_eq!(wayland_portal_timeout_ms(), wayland_portal_timeout_secs() * 1000);
        assert_eq!(pipewire_frame_timeout_ms(), pipewire_frame_timeout_secs() * 1000);
    }

    #[test]
    fn test_env_override_with_value() {
        // Test that environment variable parsing works correctly
        temp_env::with_var("SCREENSHOT_LIST_WINDOWS_TIMEOUT_MS", Some("5000"), || {
            assert_eq!(list_windows_timeout_ms(), 5000);
        });

        temp_env::with_var("SCREENSHOT_X11_CAPTURE_TIMEOUT_MS", Some("4000"), || {
            assert_eq!(x11_capture_timeout_ms(), 4000);
        });

        temp_env::with_var("SCREENSHOT_WINDOWS_CAPTURE_TIMEOUT_MS", Some("10000"), || {
            assert_eq!(windows_capture_timeout_ms(), 10000);
        });

        temp_env::with_var("SCREENSHOT_WAYLAND_PORTAL_TIMEOUT_SECS", Some("60"), || {
            assert_eq!(wayland_portal_timeout_secs(), 60);
        });

        temp_env::with_var("SCREENSHOT_PIPEWIRE_FRAME_TIMEOUT_SECS", Some("15"), || {
            assert_eq!(pipewire_frame_timeout_secs(), 15);
        });
    }

    #[test]
    fn test_env_override_invalid_value() {
        // Invalid values should fall back to default
        temp_env::with_var("SCREENSHOT_LIST_WINDOWS_TIMEOUT_MS", Some("not_a_number"), || {
            assert_eq!(list_windows_timeout_ms(), LIST_WINDOWS_TIMEOUT_MS);
        });

        temp_env::with_var("SCREENSHOT_X11_CAPTURE_TIMEOUT_MS", Some("invalid"), || {
            assert_eq!(x11_capture_timeout_ms(), X11_CAPTURE_TIMEOUT_MS);
        });

        temp_env::with_var("SCREENSHOT_WINDOWS_CAPTURE_TIMEOUT_MS", Some(""), || {
            assert_eq!(windows_capture_timeout_ms(), WINDOWS_CAPTURE_TIMEOUT_MS);
        });

        temp_env::with_var("SCREENSHOT_WAYLAND_PORTAL_TIMEOUT_SECS", Some("-1"), || {
            // Negative numbers won't parse as u64, should fall back
            assert_eq!(wayland_portal_timeout_secs(), WAYLAND_PORTAL_TIMEOUT_SECS);
        });

        temp_env::with_var("SCREENSHOT_PIPEWIRE_FRAME_TIMEOUT_SECS", Some("1.5"), || {
            // Float won't parse as u64, should fall back
            assert_eq!(pipewire_frame_timeout_secs(), PIPEWIRE_FRAME_TIMEOUT_SECS);
        });
    }
}
