//! MCP service implementation with tool routing
//!
//! This module provides the screenshot-mcp MCP server implementation
//! with the `health_check` tool for platform detection and status reporting.

use std::future::Future;

use rmcp::{
    handler::server::tool::ToolRouter,
    model::{CallToolResult, Content, ErrorData as McpError, ServerInfo},
    tool, tool_router, ServerHandler,
};

use crate::{model::HealthCheckResponse, util::detect::detect_platform};

/// Screenshot MCP server
///
/// Provides MCP tools for screenshot capture across different platforms.
/// Currently implements the `health_check` tool for M0.
#[derive(Clone)]
pub struct ScreenshotMcpServer {
    /// Tool router for dispatching tool calls
    /// Note: This field is used by the #[tool_router] macro
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl ScreenshotMcpServer {
    /// Creates a new ScreenshotMcpServer instance
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::mcp::ScreenshotMcpServer;
    ///
    /// let server = ScreenshotMcpServer::new();
    /// ```
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
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
}

impl Default for ScreenshotMcpServer {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn test_server_creation() {
        let _server = ScreenshotMcpServer::new();
        // If this compiles and runs, the server was created successfully
    }

    #[test]
    fn test_server_default() {
        let _server = ScreenshotMcpServer::default();
        // Verify default implementation works
    }

    #[tokio::test]
    async fn test_health_check_returns_success() {
        let server = ScreenshotMcpServer::new();
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
        let server = ScreenshotMcpServer::new();
        let result = server.health_check().await;

        assert!(result.is_ok(), "health_check should return Ok");

        let tool_result = result.unwrap();

        // Verify the result structure
        assert!(!tool_result.content.is_empty(), "should have content");
        assert!(!tool_result.is_error.unwrap_or(false), "should not be an error");
    }
}
