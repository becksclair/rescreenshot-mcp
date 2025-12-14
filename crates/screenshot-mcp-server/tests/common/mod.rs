//! Shared test utilities for integration and performance tests

#[cfg(target_os = "linux")]
pub mod wayland_harness;

#[cfg(target_os = "windows")]
pub mod windows_helpers;

// MCP test harness - always available (uses MockBackend)
// Allow dead_code when compiled with test binaries that don't use this module
#[allow(dead_code)]
pub mod mcp_harness;
