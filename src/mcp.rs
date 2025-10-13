//! MCP service implementation with tool routing
//!
//! This module provides the screenshot-mcp MCP server implementation
//! with tools for screenshot capture across different platforms.

use std::sync::Arc;

use rmcp::{
    handler::server::tool::ToolRouter,
    model::{CallToolResult, Content, ErrorData as McpError, ServerInfo},
    tool, tool_router, ServerHandler,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    capture::{CaptureFacade, MockBackend},
    error::CaptureError,
    model::{CaptureOptions, HealthCheckResponse, ImageFormat, WindowSelector},
    util::{
        detect::detect_platform, encode::encode_image, mcp_content::build_capture_result,
        temp_files::TempFileManager,
    },
};

/// Parameters for the capture_window tool
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CaptureWindowParams {
    /// Window title substring or regex pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_substring_or_regex: Option<String>,
    /// Window class name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Executable name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
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
#[derive(Clone)]
pub struct ScreenshotMcpServer {
    /// Tool router for dispatching tool calls
    /// Note: This field is used by the #[tool_router] macro
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    /// Backend for screenshot capture (Wayland, X11, Windows, macOS, or Mock)
    backend:     Arc<dyn CaptureFacade>,
    /// Temporary file manager for storing captured screenshots
    temp_files:  Arc<TempFileManager>,
}

#[tool_router]
impl ScreenshotMcpServer {
    /// Creates a new ScreenshotMcpServer instance with specified backend and
    /// temp file manager
    ///
    /// # Arguments
    ///
    /// * `backend` - The capture backend to use (Wayland, X11, Windows, macOS,
    ///   or Mock)
    /// * `temp_files` - The temporary file manager for storing screenshots
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    ///
    /// use screenshot_mcp::{
    ///     capture::MockBackend, mcp::ScreenshotMcpServer, util::temp_files::TempFileManager,
    /// };
    ///
    /// let backend = Arc::new(MockBackend::new());
    /// let temp_files = Arc::new(TempFileManager::new());
    /// let server = ScreenshotMcpServer::new(backend, temp_files);
    /// ```
    pub fn new(backend: Arc<dyn CaptureFacade>, temp_files: Arc<TempFileManager>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            backend,
            temp_files,
        }
    }

    /// Creates a new ScreenshotMcpServer with MockBackend for testing
    ///
    /// This is a convenience constructor that initializes the server with a
    /// MockBackend and a fresh TempFileManager. Useful for testing and
    /// development.
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::mcp::ScreenshotMcpServer;
    ///
    /// let server = ScreenshotMcpServer::new_with_mock();
    /// ```
    pub fn new_with_mock() -> Self {
        let backend = Arc::new(MockBackend::new());
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
    async fn health_check(&self) -> Result<CallToolResult, McpError> {
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
    async fn list_windows(&self) -> Result<CallToolResult, McpError> {
        // Call backend to enumerate windows
        let windows = self
            .backend
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

// Manual implementation for capture_window tool (not using #[tool] macro due to
// parameter limitations)
impl ScreenshotMcpServer {
    /// Captures a screenshot of a specific window
    ///
    /// Finds a window matching the selector criteria and captures a screenshot
    /// as PNG format with default settings. Returns both inline image data and
    /// a file resource link for persistent access.
    ///
    /// # Parameters
    ///
    /// - `title_substring_or_regex` (optional): Window title substring or regex
    ///   pattern
    /// - `class` (optional): Window class name
    /// - `exe` (optional): Executable name
    ///
    /// At least one of `title_substring_or_regex`, `class`, or `exe` must be
    /// specified.
    ///
    /// # Default Settings
    ///
    /// - Format: PNG (lossless)
    /// - Quality: 80 (for JPEG/WebP, not applicable to PNG)
    /// - Scale: 1.0 (original size)
    /// - Cursor: Not included
    ///
    /// # Returns
    ///
    /// A `CallToolResult` containing:
    /// 1. Inline image content (base64-encoded PNG)
    /// 2. Resource link with file:// URI
    /// 3. Metadata (dimensions, format, size)
    ///
    /// # Examples
    ///
    /// Request:
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
    pub async fn capture_window(
        &self,
        params: CaptureWindowParams,
    ) -> Result<CallToolResult, McpError> {
        // Build WindowSelector from parameters
        let selector = WindowSelector {
            title_substring_or_regex: params.title_substring_or_regex,
            class: params.class,
            exe: params.exe,
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

        // Use default capture options (PNG, quality 80, scale 1.0, no cursor)
        let mut opts = CaptureOptions {
            format:         ImageFormat::Png,
            quality:        80,
            scale:          1.0,
            include_cursor: false,
            region:         None,
            wayland_source: None,
        };
        opts.validate();

        // Resolve window target
        let handle = self
            .backend
            .resolve_target(&selector)
            .await
            .map_err(convert_capture_error_to_mcp)?;

        // Capture the window
        let mut image_buffer = self
            .backend
            .capture_window(handle, &opts)
            .await
            .map_err(convert_capture_error_to_mcp)?;

        // Apply scaling if needed (scale is already validated to 0.1-2.0)
        if (opts.scale - 1.0).abs() > f32::EPSILON {
            image_buffer = image_buffer
                .scale(opts.scale)
                .map_err(convert_capture_error_to_mcp)?;
        }

        // Get dimensions after scaling
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
    use crate::model::WindowInfo;

    #[test]
    fn test_server_creation_with_mock() {
        let _server = ScreenshotMcpServer::new_with_mock();
        // If this compiles and runs, the server was created successfully
    }

    #[test]
    fn test_server_creation_with_backend() {
        let backend = Arc::new(MockBackend::new());
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
                class: None,
                exe: None,
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
                class: None,
                exe: None,
            })
            .await;

        assert!(result.is_ok(), "capture_window should succeed");

        let tool_result = result.unwrap();
        let image = tool_result.content[0].as_image().unwrap();
        assert_eq!(image.mime_type, "image/png", "should use default PNG format");

        let metadata_text = tool_result.content[2].as_text().unwrap();
        assert!(metadata_text.text.contains("png"), "metadata should show png format");
    }

    #[tokio::test]
    async fn test_capture_window_with_default_scale() {
        let server = ScreenshotMcpServer::new_with_mock();

        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Terminal".to_string()),
                class: None,
                exe: None,
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
                class: None,
                exe: None,
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
        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: None,
                class: None,
                exe: None,
            })
            .await;

        assert!(result.is_err(), "should fail when no selector fields provided");

        let error = result.unwrap_err();
        let error_msg = format!("{:?}", error);
        assert!(
            error_msg.contains("must be specified"),
            "error should indicate missing selector"
        );
    }

    #[tokio::test]
    async fn test_capture_window_always_png() {
        let server = ScreenshotMcpServer::new_with_mock();

        // Simplified version always uses PNG format (no format parameter)
        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                class: None,
                exe: None,
            })
            .await;

        assert!(result.is_ok(), "capture should succeed with default PNG");

        let tool_result = result.unwrap();
        let image = tool_result.content[0].as_image().unwrap();
        assert_eq!(image.mime_type, "image/png", "should always use PNG format");
    }

    #[tokio::test]
    async fn test_temp_file_created_and_tracked() {
        let backend = Arc::new(MockBackend::new());
        let temp_files = Arc::new(TempFileManager::new());
        let server = ScreenshotMcpServer::new(backend, Arc::clone(&temp_files));

        // Verify no files initially
        assert_eq!(temp_files.count(), 0);

        // Capture a window
        let result = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                class: None,
                exe: None,
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
        let backend = Arc::new(MockBackend::new());
        let temp_files = Arc::new(TempFileManager::new());
        let server = ScreenshotMcpServer::new(backend, Arc::clone(&temp_files));

        // Capture three different windows
        let _r1 = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Firefox".to_string()),
                class: None,
                exe: None,
            })
            .await
            .unwrap();

        let _r2 = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Visual Studio Code".to_string()),
                class: None,
                exe: None,
            })
            .await
            .unwrap();

        let _r3 = server
            .capture_window(CaptureWindowParams {
                title_substring_or_regex: Some("Terminal".to_string()),
                class: None,
                exe: None,
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
                class: None,
                exe: None,
            })
            .await
            .unwrap();

        // Verify structure: image + resource link + metadata
        assert_eq!(result.content.len(), 3);

        // Content 1: Image with base64 data
        let image = result.content[0].as_image().unwrap();
        assert_eq!(image.mime_type, "image/png", "should use default PNG");
        assert!(!image.data.is_empty(), "should have base64 data");

        // Content 2: Resource link with file:// URI
        let resource = result.content[1].as_text().unwrap();
        assert!(resource.text.contains("file://"), "should have file:// URI");
        assert!(resource.text.contains("screenshot-"), "should reference screenshot file");

        // Content 3: Metadata with JSON
        let metadata = result.content[2].as_text().unwrap();
        assert!(metadata.text.contains("dimensions"), "should have dimensions");
        assert!(metadata.text.contains("png"), "should specify PNG format");
        assert!(metadata.text.contains("size_bytes"), "should include file size");
    }
}
