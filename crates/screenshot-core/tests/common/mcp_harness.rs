//! MCP Server Test Harness
//!
//! Provides reusable test fixtures for exercising the screenshot MCP server
//! with both mock and live backends.
//!
//! # Usage
//!
//! ```rust
//! use common::mcp_harness::{ContentValidator, McpTestContext};
//!
//! #[tokio::test]
//! async fn test_capture() {
//!     let ctx = McpTestContext::new_with_mock();
//!     let result = ctx.capture_window_by_title("Firefox").await.unwrap();
//!     let parts = ContentValidator::validate_capture_result(&result).unwrap();
//!     assert!(parts.image_bytes.starts_with(&[0x89, 0x50, 0x4e, 0x47])); // PNG
//! }
//! ```

use std::sync::Arc;

use base64::{Engine, engine::general_purpose::STANDARD};
use rmcp::model::CallToolResult;
#[cfg(target_os = "windows")]
use screenshot_core::capture::WindowsBackend;
use screenshot_core::{
    capture::{CaptureFacade, MockBackend},
    util::temp_files::TempFileManager,
};

/// Test fixture for MCP server integration tests
///
/// Wraps a `ScreenshotMcpServer` with convenience methods for calling MCP tools
/// and tracking temp files. Supports both headless testing with MockBackend
/// and live testing with WindowsBackend.
pub struct McpTestContext {
    /// The MCP server instance
    pub server: ScreenshotMcpServer,
    /// Shared temp file manager for cleanup tracking
    pub temp_files: Arc<TempFileManager>,
    /// Backend used by the server (for inspection if needed)
    #[allow(dead_code)]
    backend: Arc<dyn CaptureFacade>,
}

impl McpTestContext {
    /// Create test context with MockBackend for headless testing
    ///
    /// This is the preferred constructor for most tests as it doesn't
    /// require a real display environment.
    pub fn new_with_mock() -> Self {
        let backend: Arc<dyn CaptureFacade> = Arc::new(MockBackend::new());
        let temp_files = Arc::new(TempFileManager::new());
        let server = ScreenshotMcpServer::new(Arc::clone(&backend), Arc::clone(&temp_files));
        Self {
            server,
            temp_files,
            backend,
        }
    }

    /// Create test context with a configured MockBackend
    ///
    /// Use this when you need to inject errors or delays for testing
    /// specific scenarios.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::time::Duration;
    ///
    /// use screenshot_mcp::{capture::MockBackend, error::CaptureError, model::BackendType};
    ///
    /// // With delay
    /// let mock = MockBackend::new().with_delay(Duration::from_millis(100));
    /// let ctx = McpTestContext::new_with_configured_mock(mock);
    ///
    /// // With error injection
    /// let mock = MockBackend::new().with_error(CaptureError::PermissionDenied {
    ///     platform: "test".to_string(),
    ///     backend:  BackendType::None,
    /// });
    /// let ctx = McpTestContext::new_with_configured_mock(mock);
    /// ```
    pub fn new_with_configured_mock(mock: MockBackend) -> Self {
        let backend: Arc<dyn CaptureFacade> = Arc::new(mock);
        let temp_files = Arc::new(TempFileManager::new());
        let server = ScreenshotMcpServer::new(Arc::clone(&backend), Arc::clone(&temp_files));
        Self {
            server,
            temp_files,
            backend,
        }
    }

    /// Create test context with WindowsBackend for live testing
    ///
    /// Requires a real Windows desktop
    /// environment. Use `#[ignore]` attribute on tests using this
    /// constructor.
    ///
    /// # Panics
    /// Panics if WindowsBackend initialization fails (e.g., unsupported Windows
    /// version)
    #[cfg(target_os = "windows")]
    pub fn new_with_windows_backend() -> Self {
        let backend: Arc<dyn CaptureFacade> = Arc::new(
            WindowsBackend::new().expect("WindowsBackend should initialize on supported Windows"),
        );
        let temp_files = Arc::new(TempFileManager::new());
        let server = ScreenshotMcpServer::new(Arc::clone(&backend), Arc::clone(&temp_files));
        Self {
            server,
            temp_files,
            backend,
        }
    }

    // --- Tool invocation helpers ---

    /// Call health_check tool
    pub async fn health_check(&self) -> Result<CallToolResult, rmcp::model::ErrorData> {
        self.server.health_check().await
    }

    /// Call list_windows tool
    pub async fn list_windows(&self) -> Result<CallToolResult, rmcp::model::ErrorData> {
        self.server.list_windows().await
    }

    /// Call capture_window tool with full parameters
    pub async fn capture_window(
        &self,
        params: CaptureWindowParams,
    ) -> Result<CallToolResult, rmcp::model::ErrorData> {
        self.server.capture_window(params).await
    }

    /// Convenience: capture window by title substring
    pub async fn capture_window_by_title(
        &self,
        title: &str,
    ) -> Result<CallToolResult, rmcp::model::ErrorData> {
        self.capture_window(CaptureWindowParams {
            title_substring_or_regex: Some(title.to_string()),
            class: None,
            exe: None,
        })
        .await
    }

    /// Convenience: capture window by class name
    pub async fn capture_window_by_class(
        &self,
        class: &str,
    ) -> Result<CallToolResult, rmcp::model::ErrorData> {
        self.capture_window(CaptureWindowParams {
            title_substring_or_regex: None,
            class: Some(class.to_string()),
            exe: None,
        })
        .await
    }

    /// Convenience: capture window by executable name
    pub async fn capture_window_by_exe(
        &self,
        exe: &str,
    ) -> Result<CallToolResult, rmcp::model::ErrorData> {
        self.capture_window(CaptureWindowParams {
            title_substring_or_regex: None,
            class: None,
            exe: Some(exe.to_string()),
        })
        .await
    }

    // --- Utility methods ---

    /// Get count of temp files created
    pub fn temp_file_count(&self) -> usize {
        self.temp_files.count()
    }

    /// Get list of temp file paths
    pub fn temp_file_paths(&self) -> Vec<std::path::PathBuf> {
        self.temp_files.list_files()
    }

    /// Manually cleanup all temp files
    pub fn cleanup(&self) {
        self.temp_files.cleanup_all();
    }
}

impl Drop for McpTestContext {
    fn drop(&mut self) {
        // Clean up temp files when test context is dropped
        self.temp_files.cleanup_all();
    }
}

// ============================================================================
// Content Validators
// ============================================================================

/// Parsed components of a capture result
#[derive(Debug)]
pub struct CaptureResultParts {
    /// Decoded PNG/JPEG/WebP image bytes
    pub image_bytes: Vec<u8>,
    /// file:// URI extracted from resource link
    pub file_uri: String,
    /// Parsed metadata JSON
    pub metadata: serde_json::Value,
}

/// Validation utilities for MCP tool responses
///
/// Provides methods to validate the structure and content of MCP
/// tool results, particularly the 3-part capture_window response.
pub struct ContentValidator;

impl ContentValidator {
    /// Validate and decode base64 image from CallToolResult
    ///
    /// Extracts the first content item (expected to be an image) and
    /// decodes its base64 data.
    ///
    /// # Returns
    /// - `Ok(Vec<u8>)` - decoded image bytes
    /// - `Err(String)` - description of what failed
    pub fn validate_base64_image(
        result: &CallToolResult,
        expected_mime: &str,
    ) -> Result<Vec<u8>, String> {
        let image_content = result.content.first().ok_or("Missing image content")?;

        let image = image_content
            .as_image()
            .ok_or("First content is not an image")?;

        if image.mime_type != expected_mime {
            return Err(format!(
                "Expected MIME type '{}', got '{}'",
                expected_mime, image.mime_type
            ));
        }

        STANDARD
            .decode(&image.data)
            .map_err(|e| format!("Invalid base64: {}", e))
    }

    /// Validate file:// URI in resource link (second content item)
    ///
    /// # Returns
    /// - `Ok(String)` - the file:// URI
    /// - `Err(String)` - description of what failed
    pub fn validate_file_uri(result: &CallToolResult) -> Result<String, String> {
        let resource_content = result
            .content
            .get(1)
            .ok_or("Missing resource link content")?;

        let text = resource_content
            .as_text()
            .ok_or("Second content is not text")?;

        // Find file:// URI in markdown
        let uri_start = text
            .text
            .find("file://")
            .ok_or("Resource link missing file:// URI")?;

        // Extract until closing paren or end of string
        let rest = &text.text[uri_start..];
        let uri_end = rest
            .find(')')
            .or_else(|| rest.find('\n'))
            .unwrap_or(rest.len());

        Ok(rest[..uri_end].to_string())
    }

    /// Validate metadata JSON structure (third content item)
    ///
    /// Optionally checks expected dimensions and format.
    ///
    /// # Arguments
    /// - `expected_width` - if Some, validates width matches
    /// - `expected_height` - if Some, validates height matches
    /// - `expected_format` - if Some, validates format matches
    pub fn validate_metadata(
        result: &CallToolResult,
        expected_width: Option<u32>,
        expected_height: Option<u32>,
        expected_format: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let metadata_content = result.content.get(2).ok_or("Missing metadata content")?;

        let text = metadata_content
            .as_text()
            .ok_or("Third content is not text")?;

        // Extract JSON from markdown code block or raw JSON
        let json_str = if let Some(start) = text.text.find("```json") {
            let start = start + 7;
            let end = text.text[start..]
                .find("```")
                .map(|i| start + i)
                .ok_or("Unclosed JSON code block")?;
            text.text[start..end].trim()
        } else if let Some(start) = text.text.find('{') {
            // Try to find raw JSON
            let end = text.text.rfind('}').ok_or("No closing brace in metadata")?;
            &text.text[start..=end]
        } else {
            return Err("No JSON found in metadata".to_string());
        };

        let metadata: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON: {}", e))?;

        // Validate dimensions if specified
        if let Some(w) = expected_width {
            let dims = metadata["dimensions"]
                .as_array()
                .ok_or("Missing dimensions array")?;
            let actual_w = dims
                .first()
                .and_then(|v| v.as_u64())
                .ok_or("Invalid width in dimensions")?;
            if actual_w != w as u64 {
                return Err(format!("Expected width {}, got {}", w, actual_w));
            }
        }

        if let Some(h) = expected_height {
            let dims = metadata["dimensions"]
                .as_array()
                .ok_or("Missing dimensions array")?;
            let actual_h = dims
                .get(1)
                .and_then(|v| v.as_u64())
                .ok_or("Invalid height in dimensions")?;
            if actual_h != h as u64 {
                return Err(format!("Expected height {}, got {}", h, actual_h));
            }
        }

        if let Some(fmt) = expected_format {
            let actual_fmt = metadata["format"].as_str().ok_or("Missing format field")?;
            if actual_fmt != fmt {
                return Err(format!("Expected format '{}', got '{}'", fmt, actual_fmt));
            }
        }

        Ok(metadata)
    }

    /// Validate complete capture result structure
    ///
    /// Verifies the result has exactly 3 content items in the correct order:
    /// 1. Image (base64 PNG)
    /// 2. Resource link (markdown with file:// URI)
    /// 3. Metadata (JSON with dimensions, format, size)
    ///
    /// # Returns
    /// - `Ok(CaptureResultParts)` - parsed components for further assertions
    /// - `Err(String)` - description of what failed
    pub fn validate_capture_result(result: &CallToolResult) -> Result<CaptureResultParts, String> {
        // Must have 3 content items
        if result.content.len() != 3 {
            return Err(format!("Expected 3 content items, got {}", result.content.len()));
        }

        // Must not be an error
        if result.is_error.unwrap_or(false) {
            return Err("Result is marked as error".to_string());
        }

        let image_bytes = Self::validate_base64_image(result, "image/png")?;
        let file_uri = Self::validate_file_uri(result)?;
        let metadata = Self::validate_metadata(result, None, None, None)?;

        Ok(CaptureResultParts {
            image_bytes,
            file_uri,
            metadata,
        })
    }

    /// Verify PNG magic bytes
    ///
    /// PNG files start with: 0x89 0x50 0x4E 0x47 0x0D 0x0A 0x1A 0x0A
    pub fn is_valid_png(bytes: &[u8]) -> bool {
        bytes.len() >= 8 && bytes.starts_with(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])
    }
}

// ============================================================================
// Health Check Parser
// ============================================================================

/// Parsed health check response
#[derive(Debug, serde::Deserialize)]
pub struct HealthCheckParsed {
    pub platform: String,
    pub backend: String,
    pub ok: bool,
}

/// Parse health_check tool response
pub fn parse_health_check(result: &CallToolResult) -> Result<HealthCheckParsed, String> {
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .ok_or("Missing health check text content")?;

    serde_json::from_str(&text.text).map_err(|e| format!("Invalid health check JSON: {}", e))
}

// ============================================================================
// Window List Parser
// ============================================================================

/// Parse list_windows tool response
pub fn parse_window_list(result: &CallToolResult) -> Result<Vec<serde_json::Value>, String> {
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .ok_or("Missing window list text content")?;

    serde_json::from_str(&text.text).map_err(|e| format!("Invalid window list JSON: {}", e))
}
