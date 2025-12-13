//! Shared test utilities for integration and performance tests

#[cfg(feature = "linux-wayland")]
pub mod wayland_harness;

#[cfg(feature = "windows-backend")]
pub mod windows_helpers;

// MCP test harness - always available (uses MockBackend)
// Allow dead_code when compiled with test binaries that don't use this module
#[allow(dead_code)]
pub mod mcp_harness;
