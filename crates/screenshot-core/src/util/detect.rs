//! Platform detection logic for screenshot-mcp
//!
//! This module provides functionality to detect the operating system and
//! display backend (Wayland, X11, Windows, macOS) at runtime.

use std::env;

use crate::model::{BackendType, PlatformInfo};

/// Detects the current platform and display backend
///
/// # Returns
///
/// A `PlatformInfo` containing:
/// - `os`: The operating system name ("linux", "windows", "macos", "unknown")
/// - `backend`: The detected display backend
///
/// # Platform-specific behavior
///
/// ## Linux
/// - Checks `$WAYLAND_DISPLAY` environment variable first
/// - If set, returns `BackendType::Wayland`
/// - Otherwise checks `$DISPLAY` for X11
/// - If set, returns `BackendType::X11`
/// - If neither is set, returns `BackendType::None`
///
/// ## Windows
/// - Always returns `BackendType::Windows`
///
/// ## macOS
/// - Always returns `BackendType::MacOS`
///
/// ## Other platforms
/// - Returns `BackendType::None`
///
/// # Examples
///
/// ```
/// use screenshot_core::util::detect::detect_platform;
///
/// let platform = detect_platform();
/// println!("Running on: {} with backend: {}", platform.os, platform.backend);
/// ```
pub fn detect_platform() -> PlatformInfo {
    detect_platform_with_env(|key| env::var(key).ok())
}

/// Internal function for platform detection with custom environment variable
/// provider
///
/// This allows for easier testing by injecting mock environment variables.
fn detect_platform_with_env<F>(_env_provider: F) -> PlatformInfo
where
    F: Fn(&str) -> Option<String>,
{
    #[cfg(target_os = "linux")]
    {
        let os = "linux".to_string();
        let backend = detect_linux_backend(&_env_provider);
        PlatformInfo::new(os, backend)
    }

    #[cfg(target_os = "windows")]
    {
        let os = "windows".to_string();
        let backend = BackendType::Windows;
        PlatformInfo::new(os, backend)
    }

    #[cfg(target_os = "macos")]
    {
        let os = "macos".to_string();
        let backend = BackendType::MacOS;
        PlatformInfo::new(os, backend)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let os = "unknown".to_string();
        let backend = BackendType::None;
        PlatformInfo::new(os, backend)
    }
}

/// Detects the Linux display backend (Wayland or X11)
#[cfg(target_os = "linux")]
fn detect_linux_backend<F>(env_provider: &F) -> BackendType
where
    F: Fn(&str) -> Option<String>,
{
    // Check for Wayland first
    if let Some(wayland_display) = env_provider("WAYLAND_DISPLAY") {
        if !wayland_display.is_empty() {
            return BackendType::Wayland;
        }
    }

    // Fall back to X11 check
    if let Some(x_display) = env_provider("DISPLAY") {
        if !x_display.is_empty() {
            return BackendType::X11;
        }
    }

    // No display backend detected
    BackendType::None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// Helper function to create a mock environment provider
    fn mock_env(vars: HashMap<String, String>) -> impl Fn(&str) -> Option<String> {
        move |key: &str| vars.get(key).cloned()
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_detect_wayland() {
        let mut env_vars = HashMap::new();
        env_vars.insert("WAYLAND_DISPLAY".to_string(), "wayland-0".to_string());

        let platform = detect_platform_with_env(mock_env(env_vars));

        assert_eq!(platform.os, "linux");
        assert_eq!(platform.backend, BackendType::Wayland);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_detect_x11() {
        let mut env_vars = HashMap::new();
        env_vars.insert("DISPLAY".to_string(), ":0".to_string());

        let platform = detect_platform_with_env(mock_env(env_vars));

        assert_eq!(platform.os, "linux");
        assert_eq!(platform.backend, BackendType::X11);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_wayland_takes_precedence_over_x11() {
        let mut env_vars = HashMap::new();
        env_vars.insert("WAYLAND_DISPLAY".to_string(), "wayland-0".to_string());
        env_vars.insert("DISPLAY".to_string(), ":0".to_string());

        let platform = detect_platform_with_env(mock_env(env_vars));

        assert_eq!(platform.os, "linux");
        assert_eq!(platform.backend, BackendType::Wayland);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_detect_no_backend() {
        let env_vars = HashMap::new();
        let platform = detect_platform_with_env(mock_env(env_vars));

        assert_eq!(platform.os, "linux");
        assert_eq!(platform.backend, BackendType::None);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_empty_env_vars_treated_as_none() {
        let mut env_vars = HashMap::new();
        env_vars.insert("WAYLAND_DISPLAY".to_string(), "".to_string());
        env_vars.insert("DISPLAY".to_string(), "".to_string());

        let platform = detect_platform_with_env(mock_env(env_vars));

        assert_eq!(platform.os, "linux");
        assert_eq!(platform.backend, BackendType::None);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_detect_windows() {
        let env_vars = HashMap::new();
        let platform = detect_platform_with_env(mock_env(env_vars));

        assert_eq!(platform.os, "windows");
        assert_eq!(platform.backend, BackendType::Windows);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_detect_macos() {
        let env_vars = HashMap::new();
        let platform = detect_platform_with_env(mock_env(env_vars));

        assert_eq!(platform.os, "macos");
        assert_eq!(platform.backend, BackendType::MacOS);
    }

    #[test]
    fn test_detect_platform_public_api() {
        // Test that the public API works (actual environment)
        let platform = detect_platform();

        // Should return a valid platform
        assert!(!platform.os.is_empty());

        // Verify it's one of the expected OS values
        assert!(
            platform.os == "linux"
                || platform.os == "windows"
                || platform.os == "macos"
                || platform.os == "unknown"
        );
    }
}
