//! MCP service implementation with tool routing
//!
//! This module provides the screenshot-mcp MCP server implementation
//! with tools for screenshot capture across different platforms.

use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::tool::ToolRouter,
    model::{CallToolResult, Content, ErrorData as McpError, ServerInfo},
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::mcp_content::build_capture_result;
use screenshot_core::{
    capture::{CompositeBackend, MockBackend, composite_from_mock},
    error::CaptureError,
    model::{CaptureOptions, HealthCheckResponse, ImageFormat, SourceType, WindowSelector},
    util::{detect::detect_platform, encode::encode_image, temp_files::TempFileManager},
};

/// Output image format for screenshot capture
///
/// Defaults to `Webp` for optimal agent consumption (good compression, widely supported).
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CaptureFormat {
    /// PNG format (lossless, larger files)
    Png,
    /// JPEG format (lossy, quality-controlled)
    Jpeg,
    /// WebP format (modern, efficient compression) - default
    #[default]
    Webp,
}

impl CaptureFormat {
    /// Convert to core ImageFormat
    pub fn to_image_format(self) -> ImageFormat {
        match self {
            CaptureFormat::Png => ImageFormat::Png,
            CaptureFormat::Jpeg => ImageFormat::Jpeg,
            CaptureFormat::Webp => ImageFormat::Webp,
        }
    }
}

/// Region of a window to capture (crop coordinates)
///
/// All coordinates are in pixels relative to the window's top-left corner.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaptureRegion {
    /// X offset from left edge (pixels)
    pub x: u32,
    /// Y offset from top edge (pixels)
    pub y: u32,
    /// Width of region to capture (pixels)
    pub width: u32,
    /// Height of region to capture (pixels)
    pub height: u32,
}

impl CaptureRegion {
    /// Convert to core Region type
    pub fn to_region(self) -> screenshot_core::model::Region {
        screenshot_core::model::Region {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}

/// Parameters for the capture_window tool
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaptureWindowParams {
    // --- Window selection (at least one required) ---
    /// Window title substring or regex pattern
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_substring_or_regex: Option<String>,
    /// Window class name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Executable name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,

    // --- Capture options (all optional with defaults) ---
    /// Output image format (default: webp)
    #[serde(default)]
    pub format: CaptureFormat,

    /// Image quality for JPEG/WebP (0-100, default: 80)
    /// Ignored for PNG format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<u8>,

    /// Scale factor (0.1-2.0, default: 1.0)
    /// Values < 1.0 reduce size, > 1.0 enlarge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,

    /// Whether to include cursor in capture (default: false)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_cursor: Option<bool>,

    /// Region to capture (crop). If omitted, captures full window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<CaptureRegion>,
}

/// Parameters for the prime_wayland_consent tool
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrimeWaylandConsentParams {
    /// Type of content to capture: "monitor", "window", or "virtual"
    /// Default: "monitor"
    #[serde(default = "default_source_type")]
    pub source_type: String,

    /// Stable identifier for this source (e.g., "wayland-default",
    /// "firefox-dev") Default: "wayland-default"
    #[serde(default = "default_source_id")]
    pub source_id: String,

    /// Whether to include cursor in captures
    /// Default: false
    #[serde(default)]
    pub include_cursor: bool,
}

fn default_source_type() -> String {
    "monitor".to_string()
}

fn default_source_id() -> String {
    "wayland-default".to_string()
}

/// Parses a source type string to SourceType enum
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_source_type(source_type_str: &str) -> Result<SourceType, String> {
    match source_type_str.to_lowercase().as_str() {
        "monitor" => Ok(SourceType::Monitor),
        "window" => Ok(SourceType::Window),
        "virtual" => Ok(SourceType::Virtual),
        _ => Err(format!(
            "Invalid source_type '{}'. Must be 'monitor', 'window', or 'virtual'",
            source_type_str
        )),
    }
}

/// Converts a CaptureError to an MCP ErrorData
///
/// Maps screenshot capture errors to appropriate MCP error codes with
/// user-friendly messages and remediation hints.
fn convert_capture_error_to_mcp(error: CaptureError) -> McpError {
    match &error {
        CaptureError::WindowNotFound { .. } => McpError::invalid_params(format!("{}", error), None),
        CaptureError::PortalUnavailable { .. } => {
            McpError::internal_error(format!("{}", error), None)
        }
        CaptureError::PermissionDenied { .. } => {
            McpError::internal_error(format!("{}", error), None)
        }
        CaptureError::EncodingFailed { .. } => McpError::internal_error(format!("{}", error), None),
        CaptureError::CaptureTimeout { .. } => McpError::internal_error(format!("{}", error), None),
        CaptureError::InvalidParameter { parameter, reason } => {
            McpError::invalid_params(format!("Invalid parameter '{}': {}", parameter, reason), None)
        }
        CaptureError::BackendNotAvailable { .. } => {
            McpError::internal_error(format!("{}", error), None)
        }
        CaptureError::IoError(_) => McpError::internal_error(format!("{}", error), None),
        CaptureError::ImageError(_) => McpError::internal_error(format!("{}", error), None),
        CaptureError::KeyringUnavailable { .. } => {
            McpError::internal_error(format!("{}", error), None)
        }
        CaptureError::KeyringOperationFailed { .. } => {
            McpError::internal_error(format!("{}", error), None)
        }
        CaptureError::TokenNotFound { .. } => McpError::invalid_params(format!("{}", error), None),
        CaptureError::EncryptionFailed { .. } => {
            McpError::internal_error(format!("{}", error), None)
        }
        CaptureError::UnsupportedWindowsVersion { .. } => {
            McpError::internal_error(format!("{}", error), None)
        }
        CaptureError::WindowClosed => McpError::invalid_params(format!("{}", error), None),
        CaptureError::NotSupported { .. } => McpError::internal_error(format!("{}", error), None),
    }
}

/// Screenshot MCP server
///
/// Provides MCP tools for screenshot capture across different platforms.
///
/// # Tools
///
/// - `health_check`: Platform detection and server health status
/// - `list_windows`: Enumerate all capturable windows
/// - `capture_window`: Capture a screenshot of a specific window
/// - `prime_wayland_consent`: (Wayland only) Prime consent for headless capture
#[derive(Clone)]
pub struct ScreenshotMcpServer {
    /// Tool router for dispatching tool calls
    /// Note: This field is used by the #[tool_router] macro
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    /// Backend for screenshot capture with typed capability access
    ///
    /// Using `CompositeBackend` provides type-safe access to capabilities:
    /// - `backend.enumerator` for window enumeration (not available on Wayland)
    /// - `backend.resolver` for window resolution
    /// - `backend.capture` for screenshot capture
    /// - `backend.wayland_restore` for Wayland restore token workflow
    backend: Arc<CompositeBackend>,
    /// Temporary file manager for storing captured screenshots
    temp_files: Arc<TempFileManager>,
}

#[tool_router]
impl ScreenshotMcpServer {
    /// Creates a new ScreenshotMcpServer instance with specified backend and
    /// temp file manager
    ///
    /// # Arguments
    ///
    /// * `backend` - The composite backend providing type-safe capability access
    /// * `temp_files` - The temporary file manager for storing screenshots
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use screenshot_core::{
    ///     capture::{composite_from_mock, MockBackend},
    ///     util::temp_files::TempFileManager,
    /// };
    /// use screenshot_mcp_server::mcp::ScreenshotMcpServer;
    ///
    /// let mock = Arc::new(MockBackend::new());
    /// let backend = Arc::new(composite_from_mock(mock));
    /// let temp_files = Arc::new(TempFileManager::new());
    /// let server = ScreenshotMcpServer::new(backend, temp_files);
    /// ```
    pub fn new(backend: Arc<CompositeBackend>, temp_files: Arc<TempFileManager>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            backend,
            temp_files,
        }
    }

    /// Creates a new ScreenshotMcpServer with MockBackend for testing
    ///
    /// This is a convenience constructor that initializes the server with a
    /// MockBackend wrapped in a CompositeBackend and a fresh TempFileManager.
    /// Useful for testing and development.
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp_server::mcp::ScreenshotMcpServer;
    ///
    /// let server = ScreenshotMcpServer::new_with_mock();
    /// ```
    pub fn new_with_mock() -> Self {
        let mock = Arc::new(MockBackend::new());
        let backend = Arc::new(composite_from_mock(mock));
        let temp_files = Arc::new(TempFileManager::new());
        Self::new(backend, temp_files)
    }

    /// Health check tool - verifies server status and detects platform/backend
    ///
    /// This tool:
    /// - Detects the current operating system (Linux, Windows, macOS)
    /// - Identifies the display backend (Wayland, X11, Windows, macOS, or None)
    /// - Returns a status indicating if the server is operational
    ///
    /// # Returns
    ///
    /// A `CallToolResult` containing a JSON object with:
    /// - `platform`: The OS name ("linux", "windows", "macos", "unknown")
    /// - `backend`: The display backend ("wayland", "x11", "windows", "macos",
    ///   "none")
    /// - `ok`: Boolean indicating server health (always true unless an error
    ///   occurs)
    ///
    /// # Examples
    ///
    /// Request:
    /// ```json
    /// {
    ///   "method": "tools/call",
    ///   "params": {
    ///     "name": "health_check",
    ///     "arguments": {}
    ///   }
    /// }
    /// ```
    ///
    /// Response:
    /// ```json
    /// {
    ///   "content": [{
    ///     "type": "text",
    ///     "text": "{\"platform\":\"linux\",\"backend\":\"wayland\",\"ok\":true}"
    ///   }]
    /// }
    /// ```
    #[tool(description = "Check server health and detect platform/backend")]
    pub async fn health_check(&self) -> Result<CallToolResult, McpError> {
        // Detect the current platform and backend
        let platform_info = detect_platform();

        // Create health check response
        let response = HealthCheckResponse::from_platform(platform_info);

        // Serialize to JSON
        let json_str = serde_json::to_string(&response).map_err(|e| {
            McpError::internal_error(
                format!("Failed to serialize health check response: {}", e),
                None,
            )
        })?;

        // Return as text content wrapped in success result
        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }

    /// Lists all capturable windows on the system
    ///
    /// Returns metadata about windows that can be captured, including their
    /// IDs, titles, classes, and owning processes. The exact information
    /// available depends on the backend capabilities.
    ///
    /// # Returns
    ///
    /// A `CallToolResult` containing a JSON array of window information
    /// objects. Each object contains:
    /// - `id`: Platform-specific window identifier
    /// - `title`: Window title
    /// - `class`: Window class name
    /// - `owner`: Window owner/application name
    /// - `pid`: Process ID of the window owner
    /// - `backend`: Backend that detected this window
    ///
    /// # Examples
    ///
    /// Request:
    /// ```json
    /// {
    ///   "method": "tools/call",
    ///   "params": {
    ///     "name": "list_windows",
    ///     "arguments": {}
    ///   }
    /// }
    /// ```
    ///
    /// Response:
    /// ```json
    /// {
    ///   "content": [{
    ///     "type": "text",
    ///     "text": "[{\"id\":\"0x1\",\"title\":\"Firefox\",\"class\":\"Navigator\",\"owner\":\"firefox\",\"pid\":1234,\"backend\":\"x11\"}]"
    ///   }]
    /// }
    /// ```
    #[tool(description = "List all capturable windows on the system")]
    pub async fn list_windows(&self) -> Result<CallToolResult, McpError> {
        // Get window enumerator capability (not available on Wayland)
        let enumerator = self.backend.enumerator.as_ref().ok_or_else(|| {
            McpError::internal_error(
                "Window enumeration is not available on this backend. \
                 On Wayland, use prime_wayland_consent to select sources.",
                None,
            )
        })?;

        // Call backend to enumerate windows
        let windows = enumerator
            .list_windows()
            .await
            .map_err(convert_capture_error_to_mcp)?;

        // Serialize to JSON
        let json_str = serde_json::to_string(&windows).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize window list: {}", e), None)
        })?;

        // Return as text content
        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }
}

// Manual implementation for prime_wayland_consent tool (not using #[tool] macro
// due to feature gate limitations with tool_router)
impl ScreenshotMcpServer {
    /// Prime Wayland consent - requests permission and stores restore tokens
    ///
    /// **Wayland-only tool** that opens the XDG Desktop Portal screencast
    /// picker, requests user permission for screen capture, and stores the
    /// resulting restore tokens for future headless captures.
    ///
    /// # Workflow
    ///
    /// 1. Opens portal picker dialog (user selects screen/window)
    /// 2. User grants permission
    /// 3. Restore tokens are stored securely in KeyStore
    /// 4. Returns source IDs for use with `capture_window`
    ///
    /// # Parameters
    ///
    /// - `source_type` (optional): "monitor", "window", or "virtual" (default:
    ///   "monitor")
    /// - `source_id` (optional): Custom identifier for this source (default:
    ///   "wayland-default")
    /// - `include_cursor` (optional): Include cursor in captures (default:
    ///   false)
    ///
    /// # Returns
    ///
    /// JSON object with:
    /// - `status`: "success"
    /// - `source_id`: Primary source ID for use with capture_window
    /// - `all_source_ids`: Array of all source IDs (if multiple streams)
    /// - `num_streams`: Number of streams captured
    /// - `next_steps`: Instructions for using the stored tokens
    ///
    /// # Errors
    ///
    /// - Requires Wayland backend (fails on X11/Windows/macOS)
    /// - Portal service must be running (xdg-desktop-portal)
    /// - User must grant permission (cancelling returns error)
    /// - Times out after 30 seconds if no user response
    ///
    /// # Examples
    ///
    /// Request (minimal):
    /// ```json
    /// {
    ///   "method": "tools/call",
    ///   "params": {
    ///     "name": "prime_wayland_consent",
    ///     "arguments": {}
    ///   }
    /// }
    /// ```
    ///
    /// Request (with custom ID):
    /// ```json
    /// {
    ///   "method": "tools/call",
    ///   "params": {
    ///     "name": "prime_wayland_consent",
    ///     "arguments": {
    ///       "sourceType": "monitor",
    ///       "sourceId": "my-main-monitor",
    ///       "includeCursor": false
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// Response:
    /// ```json
    /// {
    ///   "content": [{
    ///     "type": "text",
    ///     "text": "{\"status\":\"success\",\"source_id\":\"wayland-default\",\"num_streams\":1,...}"
    ///   }]
    /// }
    /// ```
    #[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
    pub async fn prime_wayland_consent(
        &self,
        params: PrimeWaylandConsentParams,
    ) -> Result<CallToolResult, McpError> {
        // Step 1: Check for Wayland restore capability via typed field (no downcast needed!)
        let wayland_capability = self.backend.wayland_restore.as_ref().ok_or_else(|| {
            McpError::internal_error(
                "prime_wayland_consent requires Wayland backend. This tool is only \
                 available on Linux with Wayland compositor. Current backend does not \
                 support this operation.",
                None,
            )
        })?;

        // Step 2: Parse source_type string to enum
        let source_type = parse_source_type(&params.source_type)
            .map_err(|e| McpError::invalid_params(e, None))?;

        // Step 3: Call backend prime_consent via trait object
        let result = wayland_capability
            .prime_consent(source_type, &params.source_id, params.include_cursor)
            .await
            .map_err(convert_capture_error_to_mcp)?;

        // Step 4: Build structured JSON response
        let response_json = serde_json::json!({
            "status": "success",
            "source_id": result.primary_source_id,
            "all_source_ids": result.all_source_ids,
            "num_streams": result.num_streams,
            "source_type": params.source_type,
            "details": format!(
                "Permission granted for {} {}. Restore token(s) stored securely.",
                result.num_streams,
                if result.num_streams == 1 { "source" } else { "sources" }
            ),
            "next_steps": if result.num_streams == 1 {
                format!(
                    "Call capture_window with exe='wayland:{}' to capture this source.",
                    result.primary_source_id
                )
            } else {
                format!(
                    "Call capture_window with any of: {}",
                    result.all_source_ids.iter()
                        .map(|id| format!("'wayland:{}'", id))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        });

        // Step 5: Return success
        let json_str = serde_json::to_string(&response_json).map_err(|e| {
            McpError::internal_error(
                format!("Failed to serialize prime_wayland_consent response: {}", e),
                None,
            )
        })?;

        Ok(CallToolResult::success(vec![Content::text(json_str)]))
    }
}

// Manual implementation for capture_window tool (not using #[tool] macro due to
// parameter limitations)
impl ScreenshotMcpServer {
    /// Captures a screenshot of a specific window
    ///
    /// Finds a window matching the selector criteria and captures a screenshot.
    /// Returns both inline image data and a file resource link for persistent access.
    ///
    /// # Window Selection Parameters (at least one required)
    ///
    /// - `titleSubstringOrRegex` (optional): Window title substring or regex pattern
    /// - `class` (optional): Window class name
    /// - `exe` (optional): Executable name
    ///
    /// # Capture Options (all optional)
    ///
    /// - `format` (optional): Output format - "png", "jpeg", or "webp" (default: "webp")
    /// - `quality` (optional): Quality 0-100 for JPEG/WebP (default: 80, ignored for PNG)
    /// - `scale` (optional): Scale factor 0.1-2.0 (default: 1.0)
    /// - `includeCursor` (optional): Include cursor in capture (default: false)
    /// - `region` (optional): Crop region `{x, y, width, height}` (default: full window)
    ///
    /// # Returns
    ///
    /// A `CallToolResult` containing:
    /// 1. Inline image content (base64-encoded)
    /// 2. Resource link with file:// URI
    /// 3. Metadata (dimensions, format, size)
    ///
    /// # Examples
    ///
    /// Minimal request (uses defaults):
    /// ```json
    /// {
    ///   "method": "tools/call",
    ///   "params": {
    ///     "name": "capture_window",
    ///     "arguments": {
    ///       "titleSubstringOrRegex": "Firefox"
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// Full options:
    /// ```json
    /// {
    ///   "method": "tools/call",
    ///   "params": {
    ///     "name": "capture_window",
    ///     "arguments": {
    ///       "titleSubstringOrRegex": "Firefox",
    ///       "format": "png",
    ///       "quality": 95,
    ///       "scale": 0.5,
    ///       "includeCursor": true,
    ///       "region": {"x": 0, "y": 0, "width": 800, "height": 600}
    ///     }
    ///   }
    /// }
    /// ```
    pub async fn capture_window(
        &self,
        params: CaptureWindowParams,
    ) -> Result<CallToolResult, McpError> {
        // Build WindowSelector from parameters
        let selector = WindowSelector {
            title_substring_or_regex: params.title_substring_or_regex.clone(),
            class: params.class.clone(),
            exe: params.exe.clone(),
        };

        // Validate that at least one selector field is provided
        if selector.title_substring_or_regex.is_none()
            && selector.class.is_none()
            && selector.exe.is_none()
        {
            return Err(McpError::invalid_params(
                "At least one of 'title_substring_or_regex', 'class', or 'exe' must be specified",
                None,
            ));
        }

        // Validate scale if provided (0.1-2.0 range)
        if let Some(scale) = params.scale {
            if !(0.1..=2.0).contains(&scale) {
                return Err(McpError::invalid_params(
                    format!("Invalid scale '{}': must be between 0.1 and 2.0", scale),
                    None,
                ));
            }
        }

        // Validate region if provided (non-zero dimensions)
        if let Some(ref region) = params.region {
            if region.width == 0 || region.height == 0 {
                return Err(McpError::invalid_params(
                    "Invalid region: width and height must be greater than 0",
                    None,
                ));
            }
        }

        // Build capture options from params (with defaults)
        let scale = params.scale.unwrap_or(1.0);
        let quality = params.quality.unwrap_or(80);
        let include_cursor = params.include_cursor.unwrap_or(false);
        let format = params.format.to_image_format();
        let region = params.region.map(|r| r.to_region());

        let mut opts = CaptureOptions {
            format,
            quality,
            scale,
            include_cursor,
            region,
            wayland_source: None,
            max_dimension: Some(1920), // Auto-scale 4K to ~1080p for efficient transfer
        };
        opts.validate();

        // Get window resolver capability
        let resolver = self.backend.resolver.as_ref().ok_or_else(|| {
            McpError::internal_error("Window resolution is not available on this backend.", None)
        })?;

        // Resolve window target
        let handle = resolver
            .resolve(&selector)
            .await
            .map_err(convert_capture_error_to_mcp)?;

        // Capture the window using ScreenCapture capability
        let image_buffer = self
            .backend
            .capture
            .capture_window(handle, &opts)
            .await
            .map_err(convert_capture_error_to_mcp)?;

        // Note: scaling is applied by the backend via opts.scale
        // Get dimensions (already scaled if scale != 1.0)
        let dimensions = image_buffer.dimensions();

        // Encode the image
        let encoded_data =
            encode_image(&image_buffer, &opts).map_err(convert_capture_error_to_mcp)?;

        // Write to temp file
        let (file_path, _file_size) = self
            .temp_files
            .write_image(&encoded_data, opts.format)
            .map_err(convert_capture_error_to_mcp)?;

        // Build dual-format result (image + file link + metadata)
        let result = build_capture_result(&encoded_data, &file_path, &opts, dimensions);

        Ok(result)
    }
}

impl Default for ScreenshotMcpServer {
    fn default() -> Self {
        Self::new_with_mock()
    }
}

// Implement ServerHandler to make ScreenshotMcpServer a valid Service
impl ServerHandler for ScreenshotMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenshot_core::model::WindowInfo;

    #[test]
    fn test_server_creation_with_mock() {
        let _server = ScreenshotMcpServer::new_with_mock();
        // If this compiles and runs, the server was created successfully
    }

    #[test]
    fn test_server_creation_with_backend() {
        let mock = Arc::new(MockBackend::new());
        let backend = Arc::new(composite_from_mock(mock));
        let temp_files = Arc::new(TempFileManager::new());
        let _server = ScreenshotMcpServer::new(backend, temp_files);
        // Verify creation with explicit backend works
    }

    #[test]
    fn test_server_default() {
        let _server = ScreenshotMcpServer::default();
        // Verify default implementation works
    }

    #[tokio::test]
    async fn test_health_check_returns_success() {
        let server = ScreenshotMcpServer::new_with_mock();
        let result = server.health_check().await;

        // Verify the call succeeds
        assert!(result.is_ok());

        let tool_result = result.unwrap();

        // Verify it's not an error result
        assert!(!tool_result.is_error.unwrap_or(false));

        // Verify we have content
        assert!(!tool_result.content.is_empty());
    }

    #[tokio::test]
    async fn test_health_check_structure() {
        let server = ScreenshotMcpServer::new_with_mock();
        let result = server.health_check().await;

        assert!(result.is_ok(), "health_check should return Ok");

        let tool_result = result.unwrap();

        // Verify the result structure
        assert!(!tool_result.content.is_empty(), "should have content");
        assert!(!tool_result.is_error.unwrap_or(false), "should not be an error");
    }

    // ========== M1 Phase 9 Integration Tests ==========

    #[tokio::test]
    async fn test_list_windows_returns_mock_data() {
        let server = ScreenshotMcpServer::new_with_mock();
        let result = server.list_windows().await;

        assert!(result.is_ok(), "list_windows should succeed");

        let tool_result = result.unwrap();
        assert!(!tool_result.is_error.unwrap_or(false), "should not be an error");
        assert!(!tool_result.content.is_empty(), "should have content");

        // Parse JSON and verify we got 3 windows
        let content = tool_result.content[0].as_text().unwrap();
        let windows: Vec<WindowInfo> =
            serde_json::from_str(&content.text).expect("should parse as JSON");
        assert_eq!(windows.len(), 3, "MockBackend should return 3 windows");

        // Verify window names (Firefox, VSCode, Terminal)
        let titles: Vec<_> = windows.iter().map(|w| w.title.as_str()).collect();
        assert!(titles.contains(&"Mozilla Firefox"));
        assert!(titles.contains(&"Visual Studio Code"));
        assert!(titles.contains(&"Terminal - Alacritty"));
    }

    #[tokio::test]
    async fn test_capture_window_by_title_success() {
        let server = ScreenshotMcpServer::new_with_mock();

        // Capture Firefox window
        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                ..Default::default()
            })
            .await;

        assert!(result.is_ok(), "capture_window should succeed");

        let tool_result = result.unwrap();
        assert!(!tool_result.is_error.unwrap_or(false), "should not be an error");
        assert_eq!(tool_result.content.len(), 3, "should have 3 content items");

        // First content should be image
        assert!(tool_result.content[0].as_image().is_some(), "first content should be image");

        // Second content should be resource link (text)
        assert!(
            tool_result.content[1].as_text().is_some(),
            "second content should be resource link"
        );

        // Third content should be metadata (text)
        let metadata_text = tool_result.content[2].as_text().unwrap();
        assert!(metadata_text.text.contains("dimensions"), "metadata should contain dimensions");
        assert!(metadata_text.text.contains("1920"), "should have correct dimensions");
    }

    #[tokio::test]
    async fn test_capture_window_with_default_format() {
        let server = ScreenshotMcpServer::new_with_mock();

        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Visual Studio Code".to_string()),
                ..Default::default()
            })
            .await;

        assert!(result.is_ok(), "capture_window should succeed");

        let tool_result = result.unwrap();
        let image = tool_result.content[0].as_image().unwrap();
        assert_eq!(image.mime_type, "image/webp", "should use default WebP format");

        let metadata_text = tool_result.content[2].as_text().unwrap();
        assert!(metadata_text.text.contains("webp"), "metadata should show webp format");
    }

    #[tokio::test]
    async fn test_capture_window_with_default_scale() {
        let server = ScreenshotMcpServer::new_with_mock();

        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Terminal".to_string()),
                ..Default::default()
            })
            .await;

        assert!(result.is_ok(), "capture_window should succeed");

        let tool_result = result.unwrap();
        let metadata_text = tool_result.content[2].as_text().unwrap();

        // Original is 1920x1080, default scale 1.0 keeps original dimensions
        assert!(metadata_text.text.contains("1920"), "should have original width");
        assert!(metadata_text.text.contains("1080"), "should have original height");
    }

    #[tokio::test]
    async fn test_capture_window_not_found() {
        let server = ScreenshotMcpServer::new_with_mock();

        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("NonexistentWindow".to_string()),
                ..Default::default()
            })
            .await;

        assert!(result.is_err(), "should fail for nonexistent window");

        let error = result.unwrap_err();
        let error_msg = format!("{:?}", error);
        assert!(
            error_msg.contains("WindowNotFound") || error_msg.contains("not found"),
            "error should indicate window not found"
        );
    }

    #[tokio::test]
    async fn test_capture_window_missing_selector() {
        let server = ScreenshotMcpServer::new_with_mock();

        // No selector fields provided
        let result = server.capture_window(CaptureWindowParams::default()).await;

        assert!(result.is_err(), "should fail when no selector fields provided");

        let error = result.unwrap_err();
        let error_msg = format!("{:?}", error);
        assert!(
            error_msg.contains("must be specified"),
            "error should indicate missing selector"
        );
    }

    #[tokio::test]
    async fn test_capture_window_always_webp() {
        let server = ScreenshotMcpServer::new_with_mock();

        // Always uses WebP format for agent-friendly defaults
        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                ..Default::default()
            })
            .await;

        assert!(result.is_ok(), "capture should succeed with default WebP");

        let tool_result = result.unwrap();
        let image = tool_result.content[0].as_image().unwrap();
        assert_eq!(image.mime_type, "image/webp", "should always use WebP format");
    }

    #[tokio::test]
    async fn test_temp_file_created_and_tracked() {
        let mock = Arc::new(MockBackend::new());
        let backend = Arc::new(composite_from_mock(mock));
        let temp_files = Arc::new(TempFileManager::new());
        let server = ScreenshotMcpServer::new(backend, Arc::clone(&temp_files));

        // Verify no files initially
        assert_eq!(temp_files.count(), 0);

        // Capture a window
        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                ..Default::default()
            })
            .await;

        assert!(result.is_ok());

        // Verify temp file was created and tracked
        assert_eq!(temp_files.count(), 1, "should have 1 temp file");

        let files = temp_files.list_files();
        assert_eq!(files.len(), 1);
        assert!(files[0].exists(), "temp file should exist on filesystem");
    }

    #[tokio::test]
    async fn test_multiple_captures_create_unique_files() {
        let mock = Arc::new(MockBackend::new());
        let backend = Arc::new(composite_from_mock(mock));
        let temp_files = Arc::new(TempFileManager::new());
        let server = ScreenshotMcpServer::new(backend, Arc::clone(&temp_files));

        // Capture three different windows
        let _r1 = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();

        let _r2 = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Visual Studio Code".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();

        let _r3 = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Terminal".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();

        // Verify 3 unique temp files
        assert_eq!(temp_files.count(), 3, "should have 3 temp files");

        let files = temp_files.list_files();
        assert_eq!(files.len(), 3);

        // All files should be unique
        assert_ne!(files[0], files[1]);
        assert_ne!(files[1], files[2]);
        assert_ne!(files[0], files[2]);

        // All should exist
        for file in files {
            assert!(file.exists(), "each temp file should exist");
        }
    }

    #[tokio::test]
    async fn test_capture_result_dual_format_structure() {
        let server = ScreenshotMcpServer::new_with_mock();

        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();

        // Verify structure: image + resource link + metadata
        assert_eq!(result.content.len(), 3);

        // Content 1: Image with base64 data
        let image = result.content[0].as_image().unwrap();
        assert_eq!(image.mime_type, "image/webp", "should use default WebP");
        assert!(!image.data.is_empty(), "should have base64 data");

        // Content 2: Resource link with file:// URI
        let resource = result.content[1].as_text().unwrap();
        assert!(resource.text.contains("file://"), "should have file:// URI");
        assert!(resource.text.contains("screenshot-"), "should reference screenshot file");

        // Content 3: Metadata with JSON
        let metadata = result.content[2].as_text().unwrap();
        assert!(metadata.text.contains("dimensions"), "should have dimensions");
        assert!(metadata.text.contains("webp"), "should specify WebP format");
        assert!(metadata.text.contains("size_bytes"), "should include file size");
    }

    // ========== New Capture Parameter Tests ==========

    #[tokio::test]
    async fn test_capture_window_with_png_format() {
        let server = ScreenshotMcpServer::new_with_mock();

        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                format: CaptureFormat::Png,
                ..Default::default()
            })
            .await;

        assert!(result.is_ok(), "capture should succeed with PNG format");

        let tool_result = result.unwrap();
        let image = tool_result.content[0].as_image().unwrap();
        assert_eq!(image.mime_type, "image/png", "should use PNG format");

        let metadata_text = tool_result.content[2].as_text().unwrap();
        assert!(metadata_text.text.contains("png"), "metadata should show png format");
    }

    #[tokio::test]
    async fn test_capture_window_with_jpeg_format() {
        let server = ScreenshotMcpServer::new_with_mock();

        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                format: CaptureFormat::Jpeg,
                quality: Some(90),
                ..Default::default()
            })
            .await;

        assert!(result.is_ok(), "capture should succeed with JPEG format");

        let tool_result = result.unwrap();
        let image = tool_result.content[0].as_image().unwrap();
        assert_eq!(image.mime_type, "image/jpeg", "should use JPEG format");
    }

    #[tokio::test]
    async fn test_capture_window_with_custom_quality() {
        let server = ScreenshotMcpServer::new_with_mock();

        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                quality: Some(50),
                ..Default::default()
            })
            .await;

        assert!(result.is_ok(), "capture should succeed with custom quality");
    }

    #[tokio::test]
    async fn test_capture_window_with_scale() {
        let server = ScreenshotMcpServer::new_with_mock();

        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                scale: Some(0.5),
                ..Default::default()
            })
            .await;

        assert!(result.is_ok(), "capture should succeed with 0.5 scale");

        let tool_result = result.unwrap();
        let metadata_text = tool_result.content[2].as_text().unwrap();

        // Original is 1920x1080, 0.5 scale -> 960x540
        assert!(metadata_text.text.contains("960"), "should have scaled width");
        assert!(metadata_text.text.contains("540"), "should have scaled height");
    }

    #[tokio::test]
    async fn test_capture_window_scale_out_of_range_fails() {
        let server = ScreenshotMcpServer::new_with_mock();

        // Scale too low
        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                scale: Some(0.05), // below 0.1 minimum
                ..Default::default()
            })
            .await;

        assert!(result.is_err(), "should fail with scale below 0.1");

        let error = result.unwrap_err();
        let error_msg = format!("{:?}", error);
        assert!(
            error_msg.contains("scale") && error_msg.contains("0.1"),
            "error should mention scale range"
        );

        // Scale too high
        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                scale: Some(3.0), // above 2.0 maximum
                ..Default::default()
            })
            .await;

        assert!(result.is_err(), "should fail with scale above 2.0");
    }

    #[tokio::test]
    async fn test_capture_window_with_region() {
        let server = ScreenshotMcpServer::new_with_mock();

        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                region: Some(CaptureRegion {
                    x: 100,
                    y: 100,
                    width: 800,
                    height: 600,
                }),
                ..Default::default()
            })
            .await;

        assert!(result.is_ok(), "capture should succeed with region");

        let tool_result = result.unwrap();
        let metadata_text = tool_result.content[2].as_text().unwrap();

        // Region crops to 800x600
        assert!(metadata_text.text.contains("800"), "should have region width");
        assert!(metadata_text.text.contains("600"), "should have region height");
    }

    #[tokio::test]
    async fn test_capture_window_region_zero_dimensions_fails() {
        let server = ScreenshotMcpServer::new_with_mock();

        // Zero width
        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                region: Some(CaptureRegion {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 100,
                }),
                ..Default::default()
            })
            .await;

        assert!(result.is_err(), "should fail with zero width");

        let error = result.unwrap_err();
        let error_msg = format!("{:?}", error);
        assert!(
            error_msg.contains("region") || error_msg.contains("width"),
            "error should mention invalid region"
        );

        // Zero height
        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                region: Some(CaptureRegion {
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 0,
                }),
                ..Default::default()
            })
            .await;

        assert!(result.is_err(), "should fail with zero height");
    }

    #[tokio::test]
    async fn test_capture_window_with_cursor() {
        let server = ScreenshotMcpServer::new_with_mock();

        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                include_cursor: Some(true),
                ..Default::default()
            })
            .await;

        // MockBackend supports cursor, so this should succeed
        assert!(result.is_ok(), "capture should succeed with cursor");
    }

    #[tokio::test]
    async fn test_capture_window_all_options_combined() {
        let server = ScreenshotMcpServer::new_with_mock();

        // Use a region that's smaller than the mock image (1920x1080)
        // and a scale that keeps the result reasonable
        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                format: CaptureFormat::Png,
                quality: Some(95),
                scale: Some(1.0), // Keep scale at 1.0 to avoid complex interactions
                include_cursor: Some(true),
                region: Some(CaptureRegion {
                    x: 50,
                    y: 50,
                    width: 800,
                    height: 600,
                }),
                ..Default::default()
            })
            .await;

        match &result {
            Ok(_) => {}
            Err(e) => eprintln!("Error: {:?}", e),
        }

        assert!(result.is_ok(), "capture should succeed with all options");

        let tool_result = result.unwrap();
        let image = tool_result.content[0].as_image().unwrap();
        assert_eq!(image.mime_type, "image/png", "should use PNG format");
    }

    // ========== Error Path Tests for MCP Error Code Mapping ==========
    //
    // These tests verify that each CaptureError variant is correctly mapped
    // to the appropriate MCP error code (invalid_params vs internal_error).

    mod error_mapping_tests {
        use super::*;
        use rmcp::model::ErrorCode;
        use screenshot_core::model::{BackendType, WindowSelector};

        /// Standard JSON-RPC invalid params code
        const INVALID_PARAMS_CODE: i32 = -32602;
        /// Standard JSON-RPC internal error code
        const INTERNAL_ERROR_CODE: i32 = -32603;

        /// Helper to check if an MCP error is an invalid_params error
        fn is_invalid_params(error: &McpError) -> bool {
            error.code == ErrorCode(INVALID_PARAMS_CODE)
        }

        /// Helper to check if an MCP error is an internal_error
        fn is_internal_error(error: &McpError) -> bool {
            error.code == ErrorCode(INTERNAL_ERROR_CODE)
        }

        #[test]
        fn test_window_not_found_maps_to_invalid_params() {
            let error = CaptureError::WindowNotFound {
                selector: WindowSelector::by_title("NonExistent"),
            };
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_invalid_params(&mcp_error), "WindowNotFound should be invalid_params");
            assert!(
                mcp_error.message.contains("not found") || mcp_error.message.contains("Window"),
                "Error message should describe window not found"
            );
        }

        #[test]
        fn test_invalid_parameter_maps_to_invalid_params() {
            let error = CaptureError::InvalidParameter {
                parameter: "scale".to_string(),
                reason: "must be between 0.1 and 2.0".to_string(),
            };
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_invalid_params(&mcp_error), "InvalidParameter should be invalid_params");
            assert!(
                mcp_error.message.contains("scale"),
                "Error message should mention the parameter"
            );
            assert!(mcp_error.message.contains("0.1"), "Error message should include reason");
        }

        #[test]
        fn test_token_not_found_maps_to_invalid_params() {
            let error = CaptureError::TokenNotFound {
                source_id: "wayland-default".to_string(),
            };
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_invalid_params(&mcp_error), "TokenNotFound should be invalid_params");
            assert!(
                mcp_error.message.contains("wayland-default"),
                "Error message should include source_id"
            );
        }

        #[test]
        fn test_window_closed_maps_to_invalid_params() {
            let error = CaptureError::WindowClosed;
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_invalid_params(&mcp_error), "WindowClosed should be invalid_params");
        }

        #[test]
        fn test_portal_unavailable_maps_to_internal_error() {
            let error = CaptureError::PortalUnavailable {
                portal: "org.freedesktop.portal.ScreenCast".to_string(),
            };
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_internal_error(&mcp_error), "PortalUnavailable should be internal_error");
            assert!(
                mcp_error.message.contains("portal") || mcp_error.message.contains("Portal"),
                "Error message should mention portal"
            );
        }

        #[test]
        fn test_permission_denied_maps_to_internal_error() {
            let error = CaptureError::PermissionDenied {
                platform: "linux".to_string(),
                backend: BackendType::Wayland,
            };
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_internal_error(&mcp_error), "PermissionDenied should be internal_error");
        }

        #[test]
        fn test_encoding_failed_maps_to_internal_error() {
            let error = CaptureError::EncodingFailed {
                format: "png".to_string(),
                reason: "invalid image dimensions".to_string(),
            };
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_internal_error(&mcp_error), "EncodingFailed should be internal_error");
        }

        #[test]
        fn test_capture_timeout_maps_to_internal_error() {
            let error = CaptureError::CaptureTimeout { duration_ms: 5000 };
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_internal_error(&mcp_error), "CaptureTimeout should be internal_error");
        }

        #[test]
        fn test_backend_not_available_maps_to_internal_error() {
            let error = CaptureError::BackendNotAvailable {
                backend: BackendType::Wayland,
            };
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_internal_error(&mcp_error), "BackendNotAvailable should be internal_error");
        }

        #[test]
        fn test_io_error_maps_to_internal_error() {
            let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
            let error = CaptureError::IoError(io_error);
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_internal_error(&mcp_error), "IoError should be internal_error");
        }

        #[test]
        fn test_image_error_maps_to_internal_error() {
            let error = CaptureError::ImageError("corrupt PNG data".to_string());
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_internal_error(&mcp_error), "ImageError should be internal_error");
        }

        #[test]
        fn test_keyring_unavailable_maps_to_internal_error() {
            let error = CaptureError::KeyringUnavailable {
                reason: "no Secret Service daemon".to_string(),
            };
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_internal_error(&mcp_error), "KeyringUnavailable should be internal_error");
        }

        #[test]
        fn test_keyring_operation_failed_maps_to_internal_error() {
            let error = CaptureError::KeyringOperationFailed {
                operation: "store".to_string(),
                reason: "access denied".to_string(),
            };
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(
                is_internal_error(&mcp_error),
                "KeyringOperationFailed should be internal_error"
            );
        }

        #[test]
        fn test_encryption_failed_maps_to_internal_error() {
            let error = CaptureError::EncryptionFailed {
                reason: "key derivation failed".to_string(),
            };
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(is_internal_error(&mcp_error), "EncryptionFailed should be internal_error");
        }

        #[test]
        fn test_unsupported_windows_version_maps_to_internal_error() {
            let error = CaptureError::UnsupportedWindowsVersion {
                current_build: 15063,
                minimum_build: 17134,
            };
            let mcp_error = convert_capture_error_to_mcp(error);

            assert!(
                is_internal_error(&mcp_error),
                "UnsupportedWindowsVersion should be internal_error"
            );
        }

        // Verify error message content is preserved
        #[test]
        fn test_error_message_content_preserved() {
            let error = CaptureError::WindowNotFound {
                selector: WindowSelector::by_title("MyCustomWindow"),
            };
            let mcp_error = convert_capture_error_to_mcp(error);

            // Error message should contain the selector info
            assert!(
                mcp_error.message.contains("MyCustomWindow")
                    || mcp_error.message.contains("not found"),
                "Error message should preserve error details"
            );
        }
    }
}
