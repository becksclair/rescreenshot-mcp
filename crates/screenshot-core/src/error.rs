//! Error types for screenshot capture operations
//!
//! This module defines comprehensive error types with user-facing messages
//! and actionable remediation hints. Each error provides context about what
//! went wrong and suggests next steps for resolution.
//!
//! # Structured Error Hints
//!
//! In addition to human-readable remediation hints, errors provide structured
//! metadata via [`ErrorHint`] for LLM auto-recovery. This allows AI clients
//! to automatically attempt recovery actions without parsing prose.
//!
//! ```rust,ignore
//! let error = CaptureError::WindowNotFound { selector };
//! let hint = error.structured_hint();
//!
//! if let Some(tool) = hint.suggested_tool {
//!     // LLM can automatically call the suggested tool
//!     println!("Try calling: {}", tool);
//! }
//! ```

use crate::model::{BackendType, WindowSelector};
use serde::{Deserialize, Serialize};

/// Result type alias for capture operations
pub type CaptureResult<T> = Result<T, CaptureError>;

/// Structured error hint for LLM auto-recovery.
///
/// Contains machine-parseable metadata that allows AI clients to automatically
/// attempt recovery actions without parsing prose remediation text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorHint {
    /// Human-readable description of the error and how to fix it
    pub message: String,

    /// Category of recovery action to attempt
    pub recovery_action: RecoveryAction,

    /// MCP tool name to call for recovery (if applicable)
    pub suggested_tool: Option<String>,

    /// Parameters to pass to the suggested tool
    pub tool_params: Option<serde_json::Value>,

    /// Whether the error is likely transient (retry may succeed)
    pub is_transient: bool,

    /// Error category for grouping/filtering
    pub category: ErrorCategory,
}

/// Category of recovery action an LLM client can attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    /// Call a different tool to get more information
    CallTool,
    /// Retry the same operation (possibly with different parameters)
    Retry,
    /// Modify parameters and retry
    ModifyParams,
    /// Require user intervention (permission grant, etc.)
    RequireUser,
    /// No automated recovery possible
    None,
}

/// High-level error category for filtering and grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Target not found (window, display, etc.)
    NotFound,
    /// Permission or access denied
    PermissionDenied,
    /// Invalid parameters or configuration
    InvalidInput,
    /// Backend or platform not available
    Unavailable,
    /// Operation timed out
    Timeout,
    /// I/O or system error
    SystemError,
    /// Encoding or processing error
    ProcessingError,
}

/// Comprehensive error type for screenshot capture operations
///
/// Each variant includes detailed context and provides remediation hints
/// through the `remediation_hint()` method.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// Window matching the selector could not be found
    #[error("Window not found: {selector:?}")]
    WindowNotFound {
        /// The selector that failed to match any window
        selector: WindowSelector,
    },

    /// Required desktop portal is unavailable
    #[error("Desktop portal '{portal}' is unavailable")]
    PortalUnavailable {
        /// Name of the unavailable portal
        portal: String,
    },

    /// Permission denied for capture operation
    #[error("Permission denied for screenshot capture on {platform}")]
    PermissionDenied {
        /// Platform where permission was denied
        platform: String,
        /// Backend that denied permission
        backend: BackendType,
    },

    /// Image encoding failed
    #[error("Failed to encode image as {format}: {reason}")]
    EncodingFailed {
        /// Image format that failed
        format: String,
        /// Reason for encoding failure
        reason: String,
    },

    /// Capture operation timed out
    #[error("Capture operation timed out after {duration_ms}ms")]
    CaptureTimeout {
        /// Timeout duration in milliseconds
        duration_ms: u64,
    },

    /// Invalid parameter provided
    #[error("Invalid parameter '{parameter}': {reason}")]
    InvalidParameter {
        /// Name of the invalid parameter
        parameter: String,
        /// Reason why it's invalid
        reason: String,
    },

    /// Requested backend is not available
    #[error("Backend {backend} is not available on this platform")]
    BackendNotAvailable {
        /// Backend type that's unavailable
        backend: BackendType,
    },

    /// I/O error occurred
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Image processing error
    #[error("Image processing error: {0}")]
    ImageError(String),

    /// Platform keyring is unavailable
    #[error("Platform keyring unavailable: {reason}")]
    KeyringUnavailable {
        /// Reason why keyring is unavailable
        reason: String,
    },

    /// Keyring operation failed
    #[error("Keyring {operation} failed: {reason}")]
    KeyringOperationFailed {
        /// The operation that failed (e.g., "store", "retrieve", "delete")
        operation: String,
        /// Reason for the failure
        reason: String,
    },

    /// Token not found for the given source ID
    #[error("No restore token found for source '{source_id}'")]
    TokenNotFound {
        /// Source ID that has no token
        source_id: String,
    },

    /// Encryption or decryption failed
    #[error("Encryption operation failed: {reason}")]
    EncryptionFailed {
        /// Reason for encryption failure
        reason: String,
    },

    /// Windows build version too old for WGC
    #[error(
        "Windows Graphics Capture requires Windows 10 version 1803+ (build {minimum_build}), but \
         found build {current_build}"
    )]
    UnsupportedWindowsVersion {
        /// Current Windows build number
        current_build: u32,
        /// Minimum required build number
        minimum_build: u32,
    },

    /// Target window was closed during capture
    #[error("Target window was closed or became invalid during capture")]
    WindowClosed,

    /// Requested capability/feature is not supported by this backend
    #[error("Feature '{feature}' is not supported by backend {backend}")]
    NotSupported {
        /// Name of the unsupported feature
        feature: String,
        /// Backend that doesn't support the feature
        backend: BackendType,
    },
}

impl CaptureError {
    /// Returns an actionable remediation hint for this error
    ///
    /// Provides platform-specific guidance and next steps for users
    /// to resolve the error condition.
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_core::{
    ///     error::CaptureError,
    ///     model::{BackendType, WindowSelector},
    /// };
    ///
    /// let error = CaptureError::WindowNotFound {
    ///     selector: WindowSelector::by_title("Firefox"),
    /// };
    ///
    /// let hint = error.remediation_hint();
    /// assert!(hint.contains("list_windows"));
    /// ```
    pub fn remediation_hint(&self) -> &str {
        match self {
            CaptureError::WindowNotFound { .. } => {
                "Use the list_windows tool to see available windows. Check if the window title, \
                 class, or executable name is correct. Window titles may change dynamically (e.g., \
                 browser tabs)."
            }
            CaptureError::PortalUnavailable { portal } => {
                if portal.contains("ScreenCast") {
                    "Install xdg-desktop-portal and a backend like xdg-desktop-portal-gtk or \
                     xdg-desktop-portal-kde. Ensure your desktop environment is running with \
                     Wayland support."
                } else {
                    "Install the required xdg-desktop-portal package and ensure your desktop \
                     environment has portal support enabled."
                }
            }
            CaptureError::PermissionDenied { backend, .. } => match backend {
                BackendType::Wayland => {
                    "Grant screenshot permission when prompted by your desktop environment. On \
                     GNOME, check Settings > Privacy > Screen Sharing. Use Wayland restore tokens \
                     to avoid repeated permission prompts."
                }
                BackendType::X11 => {
                    "Ensure your X11 server allows screen capture. Check xhost settings if running \
                     in a restricted environment."
                }
                BackendType::Windows => {
                    "Grant screen recording permission in Windows Settings > Privacy > Screen \
                     recording. Ensure the application has necessary privileges."
                }
                BackendType::MacOS => {
                    "Grant screen recording permission in System Preferences > Security & Privacy \
                     > Privacy > Screen Recording. The application must be in the allowed list."
                }
                BackendType::None => "No backend available for screenshot capture.",
            },
            CaptureError::EncodingFailed { format, .. } => match format.as_str() {
                "webp" => {
                    "WebP encoding requires the image crate with webp feature. Try using PNG \
                     format as a fallback."
                }
                "jpeg" | "jpg" => {
                    "JPEG encoding failed. Try reducing quality parameter or using PNG format."
                }
                _ => "Image encoding failed. Try a different format (PNG, WebP, or JPEG).",
            },
            CaptureError::CaptureTimeout { .. } => {
                "The capture operation took too long. This may indicate a stuck portal dialog or \
                 unresponsive desktop environment. Try closing permission dialogs and retry. \
                 Consider using Wayland restore tokens to avoid prompts."
            }
            CaptureError::InvalidParameter { parameter, .. } => match parameter.as_str() {
                "quality" => "Quality must be between 0 and 100.",
                "scale" => "Scale must be between 0.1 and 2.0.",
                _ => "Check the parameter value against the API documentation.",
            },
            CaptureError::BackendNotAvailable { backend } => match backend {
                BackendType::Wayland => {
                    "Wayland backend not available. Ensure you're running in a Wayland session \
                     with WAYLAND_DISPLAY environment variable set."
                }
                BackendType::X11 => {
                    "X11 backend not available. Ensure you're running in an X11 session with \
                     DISPLAY environment variable set."
                }
                BackendType::Windows => "Windows backend only available on Windows OS.",
                BackendType::MacOS => "macOS backend only available on macOS.",
                BackendType::None => "No screenshot backend available on this platform.",
            },
            CaptureError::IoError(_) => {
                "An I/O error occurred. Check file permissions, disk space, and system resources."
            }
            CaptureError::ImageError(_) => {
                "Image processing failed. Ensure the image data is valid and the requested \
                 operations are supported."
            }
            CaptureError::KeyringUnavailable { .. } => {
                "Platform keyring is not available. Ensure gnome-keyring, kwallet, or similar is \
                 installed and running. To enable encrypted file storage fallback, build with the \
                 file-token-fallback feature enabled (e.g., cargo build --features file-token-fallback)."
            }
            CaptureError::KeyringOperationFailed { operation, .. } => match operation.as_str() {
                "store" => {
                    "Failed to store token in keyring. Check keyring service is running and \
                     accessible. If file-token-fallback feature is enabled, will attempt file \
                     fallback."
                }
                "retrieve" => {
                    "Failed to retrieve token from keyring. The keyring service may be locked or \
                     inaccessible. Try unlocking your keyring. If file-token-fallback feature is \
                     enabled, will check file storage."
                }
                "delete" => {
                    "Failed to delete token from keyring. This may leave stale tokens. Check \
                     keyring service permissions."
                }
                _ => "Keyring operation failed. Check keyring service status and permissions.",
            },
            CaptureError::TokenNotFound { .. } => {
                "No restore token found for this source. Run prime_wayland_consent tool first to \
                 obtain a token for headless capture."
            }
            CaptureError::EncryptionFailed { .. } => {
                "Token encryption/decryption failed. This may indicate file corruption or system \
                 configuration changes. Try calling prime_wayland_consent again."
            }
            CaptureError::UnsupportedWindowsVersion { .. } => {
                "Windows Graphics Capture requires Windows 10 version 1803 (April 2018 Update) or \
                 later. Update Windows to use screenshot capture, or use an alternative tool."
            }
            CaptureError::WindowClosed => {
                "The target window was closed or destroyed while attempting capture. Ensure the \
                 window remains open during capture, or use display capture as an alternative."
            }
            CaptureError::NotSupported { feature, backend } => match (feature.as_str(), backend) {
                ("window_enumeration", BackendType::Wayland) => {
                    "Wayland does not support window enumeration due to its security model. Use \
                     prime_wayland_consent to select a window through the desktop portal, then \
                     capture using the returned source ID."
                }
                ("window_enumeration", _) => "Window enumeration is not supported on this backend.",
                ("wayland_restore", _) => {
                    "Wayland restore tokens are only available on the Wayland backend."
                }
                _ => {
                    "This feature is not supported by the current backend. Check the backend \
                     capabilities to see which features are available."
                }
            },
        }
    }

    /// Returns a structured error hint for LLM auto-recovery.
    ///
    /// Unlike `remediation_hint()` which returns prose, this method returns
    /// machine-parseable metadata that AI clients can use to automatically
    /// attempt recovery actions.
    ///
    /// # Example
    ///
    /// ```
    /// use screenshot_core::{
    ///     error::{CaptureError, RecoveryAction},
    ///     model::WindowSelector,
    /// };
    ///
    /// let error = CaptureError::WindowNotFound {
    ///     selector: WindowSelector::by_title("Firefox"),
    /// };
    ///
    /// let hint = error.structured_hint();
    /// assert_eq!(hint.recovery_action, RecoveryAction::CallTool);
    /// assert_eq!(hint.suggested_tool.as_deref(), Some("list_windows"));
    /// ```
    pub fn structured_hint(&self) -> ErrorHint {
        match self {
            CaptureError::WindowNotFound { selector } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::CallTool,
                suggested_tool: Some("list_windows".to_string()),
                tool_params: Some(serde_json::json!({
                    "hint": "Look for windows matching the original selector",
                    "original_selector": format!("{:?}", selector),
                })),
                is_transient: false,
                category: ErrorCategory::NotFound,
            },
            CaptureError::PortalUnavailable { .. } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::RequireUser,
                suggested_tool: None,
                tool_params: None,
                is_transient: false,
                category: ErrorCategory::Unavailable,
            },
            CaptureError::PermissionDenied { backend, .. } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: match backend {
                    BackendType::Wayland => RecoveryAction::CallTool,
                    _ => RecoveryAction::RequireUser,
                },
                suggested_tool: if *backend == BackendType::Wayland {
                    Some("prime_wayland_consent".to_string())
                } else {
                    None
                },
                tool_params: None,
                is_transient: false,
                category: ErrorCategory::PermissionDenied,
            },
            CaptureError::EncodingFailed { format, .. } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::ModifyParams,
                suggested_tool: None,
                tool_params: Some(serde_json::json!({
                    "suggestion": "Try a different format",
                    "failed_format": format,
                    "alternatives": match format.as_str() {
                        "webp" => vec!["png", "jpeg"],
                        "jpeg" | "jpg" => vec!["png", "webp"],
                        _ => vec!["png", "jpeg", "webp"],
                    },
                })),
                is_transient: false,
                category: ErrorCategory::ProcessingError,
            },
            CaptureError::CaptureTimeout { duration_ms } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::Retry,
                suggested_tool: None,
                tool_params: Some(serde_json::json!({
                    "suggestion": "Increase timeout or retry",
                    "original_timeout_ms": duration_ms,
                })),
                is_transient: true,
                category: ErrorCategory::Timeout,
            },
            CaptureError::InvalidParameter { parameter, reason } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::ModifyParams,
                suggested_tool: None,
                tool_params: Some(serde_json::json!({
                    "invalid_parameter": parameter,
                    "reason": reason,
                    "valid_ranges": match parameter.as_str() {
                        "quality" => serde_json::json!({"min": 0, "max": 100}),
                        "scale" => serde_json::json!({"min": 0.1, "max": 2.0}),
                        _ => serde_json::json!(null),
                    },
                })),
                is_transient: false,
                category: ErrorCategory::InvalidInput,
            },
            CaptureError::BackendNotAvailable { backend } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::None,
                suggested_tool: None,
                tool_params: Some(serde_json::json!({
                    "unavailable_backend": format!("{:?}", backend),
                })),
                is_transient: false,
                category: ErrorCategory::Unavailable,
            },
            CaptureError::IoError(_) => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::Retry,
                suggested_tool: None,
                tool_params: None,
                is_transient: true,
                category: ErrorCategory::SystemError,
            },
            CaptureError::ImageError(_) => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::None,
                suggested_tool: None,
                tool_params: None,
                is_transient: false,
                category: ErrorCategory::ProcessingError,
            },
            CaptureError::KeyringUnavailable { .. } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::RequireUser,
                suggested_tool: None,
                tool_params: Some(serde_json::json!({
                    "suggestion": "Enable file-token-fallback feature for headless operation",
                })),
                is_transient: false,
                category: ErrorCategory::Unavailable,
            },
            CaptureError::KeyringOperationFailed { operation, .. } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::Retry,
                suggested_tool: None,
                tool_params: Some(serde_json::json!({
                    "failed_operation": operation,
                })),
                is_transient: true,
                category: ErrorCategory::SystemError,
            },
            CaptureError::TokenNotFound { source_id } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::CallTool,
                suggested_tool: Some("prime_wayland_consent".to_string()),
                tool_params: Some(serde_json::json!({
                    "source_id": source_id,
                    "hint": "Run prime_wayland_consent to obtain a token first",
                })),
                is_transient: false,
                category: ErrorCategory::NotFound,
            },
            CaptureError::EncryptionFailed { .. } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::CallTool,
                suggested_tool: Some("prime_wayland_consent".to_string()),
                tool_params: Some(serde_json::json!({
                    "hint": "Re-run prime_wayland_consent to regenerate tokens",
                })),
                is_transient: false,
                category: ErrorCategory::SystemError,
            },
            CaptureError::UnsupportedWindowsVersion {
                current_build,
                minimum_build,
            } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::None,
                suggested_tool: None,
                tool_params: Some(serde_json::json!({
                    "current_build": current_build,
                    "minimum_build": minimum_build,
                    "suggestion": "Windows update required",
                })),
                is_transient: false,
                category: ErrorCategory::Unavailable,
            },
            CaptureError::WindowClosed => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: RecoveryAction::CallTool,
                suggested_tool: Some("capture_display".to_string()),
                tool_params: Some(serde_json::json!({
                    "hint": "Use display capture as fallback, or call list_windows to find another target",
                    "alternative_tools": ["capture_display", "list_windows"],
                })),
                is_transient: true,
                category: ErrorCategory::NotFound,
            },
            CaptureError::NotSupported { feature, backend } => ErrorHint {
                message: self.remediation_hint().to_string(),
                recovery_action: match (feature.as_str(), backend) {
                    ("window_enumeration", BackendType::Wayland) => RecoveryAction::CallTool,
                    _ => RecoveryAction::None,
                },
                suggested_tool: match (feature.as_str(), backend) {
                    ("window_enumeration", BackendType::Wayland) => {
                        Some("prime_wayland_consent".to_string())
                    }
                    _ => None,
                },
                tool_params: Some(serde_json::json!({
                    "unsupported_feature": feature,
                    "backend": format!("{:?}", backend),
                })),
                is_transient: false,
                category: ErrorCategory::Unavailable,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_not_found_error_message() {
        let error = CaptureError::WindowNotFound {
            selector: WindowSelector::by_title("Firefox"),
        };

        let msg = error.to_string();
        assert!(msg.contains("Window not found"));
        assert!(msg.contains("Firefox"));
    }

    #[test]
    fn test_window_not_found_remediation() {
        let error = CaptureError::WindowNotFound {
            selector: WindowSelector::by_title("Firefox"),
        };

        let hint = error.remediation_hint();
        assert!(hint.contains("list_windows"));
        assert!(hint.contains("window title"));
    }

    #[test]
    fn test_portal_unavailable_error_message() {
        let error = CaptureError::PortalUnavailable {
            portal: "org.freedesktop.portal.ScreenCast".to_string(),
        };

        let msg = error.to_string();
        assert!(msg.contains("portal"));
        assert!(msg.contains("ScreenCast"));
    }

    #[test]
    fn test_portal_unavailable_remediation() {
        let error = CaptureError::PortalUnavailable {
            portal: "org.freedesktop.portal.ScreenCast".to_string(),
        };

        let hint = error.remediation_hint();
        assert!(hint.contains("xdg-desktop-portal"));
        assert!(hint.contains("Wayland"));
    }

    #[test]
    fn test_permission_denied_wayland() {
        let error = CaptureError::PermissionDenied {
            platform: "linux".to_string(),
            backend: BackendType::Wayland,
        };

        let msg = error.to_string();
        assert!(msg.contains("Permission denied"));
        assert!(msg.contains("linux"));

        let hint = error.remediation_hint();
        assert!(hint.contains("Grant screenshot permission"));
        assert!(hint.contains("restore tokens"));
    }

    #[test]
    fn test_permission_denied_windows() {
        let error = CaptureError::PermissionDenied {
            platform: "windows".to_string(),
            backend: BackendType::Windows,
        };

        let hint = error.remediation_hint();
        assert!(hint.contains("Windows Settings"));
        assert!(hint.contains("Screen recording"));
    }

    #[test]
    fn test_permission_denied_macos() {
        let error = CaptureError::PermissionDenied {
            platform: "macos".to_string(),
            backend: BackendType::MacOS,
        };

        let hint = error.remediation_hint();
        assert!(hint.contains("System Preferences"));
        assert!(hint.contains("Privacy"));
    }

    #[test]
    fn test_encoding_failed_error_message() {
        let error = CaptureError::EncodingFailed {
            format: "webp".to_string(),
            reason: "encoder not available".to_string(),
        };

        let msg = error.to_string();
        assert!(msg.contains("Failed to encode"));
        assert!(msg.contains("webp"));
        assert!(msg.contains("encoder not available"));
    }

    #[test]
    fn test_encoding_failed_remediation() {
        let error = CaptureError::EncodingFailed {
            format: "webp".to_string(),
            reason: "encoder not available".to_string(),
        };

        let hint = error.remediation_hint();
        assert!(hint.contains("WebP"));
        assert!(hint.contains("PNG"));
    }

    #[test]
    fn test_capture_timeout_error_message() {
        let error = CaptureError::CaptureTimeout { duration_ms: 5000 };

        let msg = error.to_string();
        assert!(msg.contains("timed out"));
        assert!(msg.contains("5000"));
    }

    #[test]
    fn test_capture_timeout_remediation() {
        let error = CaptureError::CaptureTimeout { duration_ms: 5000 };

        let hint = error.remediation_hint();
        assert!(hint.contains("too long"));
        assert!(hint.contains("permission dialogs"));
    }

    #[test]
    fn test_invalid_parameter_quality() {
        let error = CaptureError::InvalidParameter {
            parameter: "quality".to_string(),
            reason: "value 150 exceeds maximum 100".to_string(),
        };

        let msg = error.to_string();
        assert!(msg.contains("Invalid parameter"));
        assert!(msg.contains("quality"));

        let hint = error.remediation_hint();
        assert!(hint.contains("0 and 100"));
    }

    #[test]
    fn test_invalid_parameter_scale() {
        let error = CaptureError::InvalidParameter {
            parameter: "scale".to_string(),
            reason: "value 5.0 exceeds maximum 2.0".to_string(),
        };

        let hint = error.remediation_hint();
        assert!(hint.contains("0.1 and 2.0"));
    }

    #[test]
    fn test_backend_not_available_wayland() {
        let error = CaptureError::BackendNotAvailable {
            backend: BackendType::Wayland,
        };

        let msg = error.to_string();
        assert!(msg.contains("wayland"));
        assert!(msg.contains("not available"));

        let hint = error.remediation_hint();
        assert!(hint.contains("WAYLAND_DISPLAY"));
    }

    #[test]
    fn test_backend_not_available_x11() {
        let error = CaptureError::BackendNotAvailable {
            backend: BackendType::X11,
        };

        let hint = error.remediation_hint();
        assert!(hint.contains("DISPLAY"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let error: CaptureError = io_error.into();

        let msg = error.to_string();
        assert!(msg.contains("I/O error"));
    }

    #[test]
    fn test_io_error_remediation() {
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let error: CaptureError = io_error.into();

        let hint = error.remediation_hint();
        assert!(hint.contains("permissions"));
        assert!(hint.contains("disk space"));
    }

    #[test]
    fn test_image_error_message() {
        let error = CaptureError::ImageError("invalid dimensions".to_string());

        let msg = error.to_string();
        assert!(msg.contains("Image processing error"));
        assert!(msg.contains("invalid dimensions"));
    }

    #[test]
    fn test_image_error_remediation() {
        let error = CaptureError::ImageError("decode failed".to_string());

        let hint = error.remediation_hint();
        assert!(hint.contains("Image processing failed"));
    }

    #[test]
    fn test_error_debug_format() {
        let error = CaptureError::WindowNotFound {
            selector: WindowSelector::by_title("Test"),
        };

        let debug = format!("{:?}", error);
        assert!(debug.contains("WindowNotFound"));
    }

    #[test]
    fn test_unsupported_windows_version_error() {
        let error = CaptureError::UnsupportedWindowsVersion {
            current_build: 15063,
            minimum_build: 17134,
        };

        let msg = error.to_string();
        assert!(msg.contains("Windows Graphics Capture"));
        assert!(msg.contains("15063"));
        assert!(msg.contains("17134"));

        let hint = error.remediation_hint();
        assert!(hint.contains("Windows 10 version 1803"));
        assert!(hint.contains("Update Windows"));
    }

    #[test]
    fn test_window_closed_error() {
        let error = CaptureError::WindowClosed;

        let msg = error.to_string();
        assert!(msg.contains("closed"));
        assert!(msg.contains("invalid"));

        let hint = error.remediation_hint();
        assert!(hint.contains("closed or destroyed"));
        assert!(hint.contains("display capture"));
    }

    // ========== Structured Hint Tests ==========

    #[test]
    fn test_structured_hint_window_not_found() {
        let error = CaptureError::WindowNotFound {
            selector: WindowSelector::by_title("Firefox"),
        };

        let hint = error.structured_hint();
        assert_eq!(hint.recovery_action, RecoveryAction::CallTool);
        assert_eq!(hint.suggested_tool.as_deref(), Some("list_windows"));
        assert!(!hint.is_transient);
        assert_eq!(hint.category, ErrorCategory::NotFound);
        assert!(hint.tool_params.is_some());
    }

    #[test]
    fn test_structured_hint_permission_denied_wayland() {
        let error = CaptureError::PermissionDenied {
            platform: "linux".to_string(),
            backend: BackendType::Wayland,
        };

        let hint = error.structured_hint();
        assert_eq!(hint.recovery_action, RecoveryAction::CallTool);
        assert_eq!(hint.suggested_tool.as_deref(), Some("prime_wayland_consent"));
        assert_eq!(hint.category, ErrorCategory::PermissionDenied);
    }

    #[test]
    fn test_structured_hint_permission_denied_windows() {
        let error = CaptureError::PermissionDenied {
            platform: "windows".to_string(),
            backend: BackendType::Windows,
        };

        let hint = error.structured_hint();
        assert_eq!(hint.recovery_action, RecoveryAction::RequireUser);
        assert!(hint.suggested_tool.is_none());
        assert_eq!(hint.category, ErrorCategory::PermissionDenied);
    }

    #[test]
    fn test_structured_hint_capture_timeout() {
        let error = CaptureError::CaptureTimeout { duration_ms: 5000 };

        let hint = error.structured_hint();
        assert_eq!(hint.recovery_action, RecoveryAction::Retry);
        assert!(hint.is_transient);
        assert_eq!(hint.category, ErrorCategory::Timeout);
    }

    #[test]
    fn test_structured_hint_invalid_parameter() {
        let error = CaptureError::InvalidParameter {
            parameter: "quality".to_string(),
            reason: "value exceeds max".to_string(),
        };

        let hint = error.structured_hint();
        assert_eq!(hint.recovery_action, RecoveryAction::ModifyParams);
        assert_eq!(hint.category, ErrorCategory::InvalidInput);
        assert!(hint.tool_params.is_some());

        // Check that valid_ranges is included
        let params = hint.tool_params.unwrap();
        assert!(params.get("valid_ranges").is_some());
    }

    #[test]
    fn test_structured_hint_encoding_failed() {
        let error = CaptureError::EncodingFailed {
            format: "webp".to_string(),
            reason: "encoder error".to_string(),
        };

        let hint = error.structured_hint();
        assert_eq!(hint.recovery_action, RecoveryAction::ModifyParams);
        assert_eq!(hint.category, ErrorCategory::ProcessingError);

        // Check alternatives are provided
        let params = hint.tool_params.unwrap();
        let alternatives = params.get("alternatives").unwrap().as_array().unwrap();
        assert!(alternatives.contains(&serde_json::json!("png")));
    }

    #[test]
    fn test_structured_hint_window_closed() {
        let error = CaptureError::WindowClosed;

        let hint = error.structured_hint();
        assert_eq!(hint.recovery_action, RecoveryAction::CallTool);
        assert_eq!(hint.suggested_tool.as_deref(), Some("capture_display"));
        assert!(hint.is_transient);
        assert_eq!(hint.category, ErrorCategory::NotFound);
    }

    #[test]
    fn test_structured_hint_serialization() {
        let error = CaptureError::WindowNotFound {
            selector: WindowSelector::by_title("Test"),
        };

        let hint = error.structured_hint();
        let json = serde_json::to_string(&hint).unwrap();

        // Verify it serializes correctly
        assert!(json.contains("list_windows"));
        assert!(json.contains("call_tool")); // snake_case from serde rename
        assert!(json.contains("not_found")); // snake_case from serde rename
    }
}
