//! Data models and type definitions for screenshot-mcp
//!
//! This module defines the core types used throughout the application:
//! - Platform and backend detection types
//! - Health check response structures
//! - Serialization/deserialization support for MCP protocol

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Represents the display backend type for the current platform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    /// No backend detected or not yet initialized
    None,
    /// Wayland display server (Linux)
    Wayland,
    /// X11 display server (Linux)
    X11,
    /// Windows Graphics Capture API
    Windows,
    /// macOS ScreenCaptureKit
    #[serde(rename = "macos")]
    MacOS,
}

impl BackendType {
    /// Returns the backend type as a lowercase string
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendType::None => "none",
            BackendType::Wayland => "wayland",
            BackendType::X11 => "x11",
            BackendType::Windows => "windows",
            BackendType::MacOS => "macos",
        }
    }
}

impl std::fmt::Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Platform information including OS and display backend
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlatformInfo {
    /// Operating system name (e.g., "linux", "windows", "macos")
    pub os:      String,
    /// Detected display backend
    pub backend: BackendType,
}

impl PlatformInfo {
    /// Creates a new PlatformInfo instance
    pub fn new(os: String, backend: BackendType) -> Self {
        Self { os, backend }
    }
}

/// Response structure for the health_check MCP tool
///
/// This is returned by the `health_check` tool to indicate server status
/// and platform detection results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HealthCheckResponse {
    /// Platform/OS name
    pub platform: String,
    /// Backend type as a string
    pub backend:  String,
    /// Whether the server is functioning correctly
    pub ok:       bool,
}

impl HealthCheckResponse {
    /// Creates a successful health check response from platform info
    pub fn from_platform(info: PlatformInfo) -> Self {
        Self {
            platform: info.os,
            backend:  info.backend.as_str().to_string(),
            ok:       true,
        }
    }

    /// Creates a health check response indicating an error state
    pub fn error(platform: String, backend: String) -> Self {
        Self {
            platform,
            backend,
            ok: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_type_serialization() {
        // Test that BackendType serializes to lowercase strings
        assert_eq!(serde_json::to_string(&BackendType::None).unwrap(), r#""none""#);
        assert_eq!(serde_json::to_string(&BackendType::Wayland).unwrap(), r#""wayland""#);
        assert_eq!(serde_json::to_string(&BackendType::X11).unwrap(), r#""x11""#);
        assert_eq!(serde_json::to_string(&BackendType::Windows).unwrap(), r#""windows""#);
        assert_eq!(serde_json::to_string(&BackendType::MacOS).unwrap(), r#""macos""#);
    }

    #[test]
    fn test_backend_type_deserialization() {
        // Test that lowercase strings deserialize to BackendType
        assert_eq!(serde_json::from_str::<BackendType>(r#""none""#).unwrap(), BackendType::None);
        assert_eq!(
            serde_json::from_str::<BackendType>(r#""wayland""#).unwrap(),
            BackendType::Wayland
        );
        assert_eq!(serde_json::from_str::<BackendType>(r#""x11""#).unwrap(), BackendType::X11);
        assert_eq!(
            serde_json::from_str::<BackendType>(r#""windows""#).unwrap(),
            BackendType::Windows
        );
        assert_eq!(serde_json::from_str::<BackendType>(r#""macos""#).unwrap(), BackendType::MacOS);
    }

    #[test]
    fn test_backend_type_as_str() {
        assert_eq!(BackendType::None.as_str(), "none");
        assert_eq!(BackendType::Wayland.as_str(), "wayland");
        assert_eq!(BackendType::X11.as_str(), "x11");
        assert_eq!(BackendType::Windows.as_str(), "windows");
        assert_eq!(BackendType::MacOS.as_str(), "macos");
    }

    #[test]
    fn test_backend_type_display() {
        assert_eq!(format!("{}", BackendType::None), "none");
        assert_eq!(format!("{}", BackendType::Wayland), "wayland");
        assert_eq!(format!("{}", BackendType::X11), "x11");
    }

    #[test]
    fn test_platform_info_serialization() {
        let info = PlatformInfo::new("linux".to_string(), BackendType::Wayland);
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains(r#""os":"linux""#));
        assert!(json.contains(r#""backend":"wayland""#));
    }

    #[test]
    fn test_platform_info_deserialization() {
        let json = r#"{"os":"linux","backend":"wayland"}"#;
        let info: PlatformInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.os, "linux");
        assert_eq!(info.backend, BackendType::Wayland);
    }

    #[test]
    fn test_health_check_response_from_platform() {
        let info = PlatformInfo::new("linux".to_string(), BackendType::X11);
        let response = HealthCheckResponse::from_platform(info);

        assert_eq!(response.platform, "linux");
        assert_eq!(response.backend, "x11");
        assert!(response.ok);
    }

    #[test]
    fn test_health_check_response_serialization() {
        let response = HealthCheckResponse {
            platform: "linux".to_string(),
            backend:  "wayland".to_string(),
            ok:       true,
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["platform"], "linux");
        assert_eq!(json["backend"], "wayland");
        assert_eq!(json["ok"], true);
    }

    #[test]
    fn test_health_check_response_deserialization() {
        let json = r#"{"platform":"windows","backend":"windows","ok":true}"#;
        let response: HealthCheckResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.platform, "windows");
        assert_eq!(response.backend, "windows");
        assert!(response.ok);
    }

    #[test]
    fn test_health_check_response_error_state() {
        let response = HealthCheckResponse::error("linux".to_string(), "none".to_string());

        assert_eq!(response.platform, "linux");
        assert_eq!(response.backend, "none");
        assert!(!response.ok);
    }

    #[test]
    fn test_json_schema_generation() {
        // Verify that types implement JsonSchema
        let _backend_schema = schemars::schema_for!(BackendType);
        let _platform_schema = schemars::schema_for!(PlatformInfo);
        let _health_schema = schemars::schema_for!(HealthCheckResponse);
    }
}
