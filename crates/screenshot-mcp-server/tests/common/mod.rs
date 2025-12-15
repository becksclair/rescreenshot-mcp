//! Shared test utilities for MCP server integration tests
//!
//! Platform-specific test helpers are available from the screenshot_test_utils crate

// MCP test harness - always available (uses MockBackend)
// Allow dead_code when compiled with test binaries that don't use this module
#[allow(dead_code)]
pub mod mcp_harness;
