//! screenshot-mcp: Cross-platform screenshot MCP server
//!
//! This library provides Model Context Protocol (MCP) server functionality
//! for capturing screenshots across different platforms (Linux Wayland/X11,
//! Windows, macOS).

#[cfg(feature = "image-processing")]
pub mod capture;
pub mod error;
pub mod mcp;
pub mod model;
#[cfg(any(feature = "perf-tests", test))]
pub mod perf;
pub mod util;
