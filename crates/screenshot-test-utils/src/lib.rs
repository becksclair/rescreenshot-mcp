//! Test utilities for screenshot-mcp integration tests
//!
//! This crate provides shared test utilities for integration testing across
//! the screenshot-mcp workspace. It consolidates platform-specific helpers
//! and cross-platform timing utilities into a single, well-documented package.
//!
//! # Usage
//!
//! Add to your crate's dev-dependencies:
//!
//! ```toml
//! [dev-dependencies]
//! screenshot-test-utils = { path = "../screenshot-test-utils" }
//! ```
//!
//! # Modules
//!
//! - [`timing`]: Cross-platform timing and performance measurement utilities
//! - [`windows`]: Windows-specific test fixtures (Windows only)
//! - [`wayland`]: Linux/Wayland test harness (Linux only)
//!
//! # Platform-Specific Examples
//!
//! ## Windows
//!
//! ```ignore
//! use screenshot_test_utils::windows::{WindowsTestContext, save_test_image};
//! use screenshot_core::model::CaptureOptions;
//!
//! #[tokio::test]
//! async fn test_window_capture() {
//!     let ctx = WindowsTestContext::new().await;
//!     let window = ctx.find_best_window().expect("No windows");
//!
//!     let image = ctx
//!         .capture_window(&window.handle, &CaptureOptions::default())
//!         .await
//!         .expect("Capture failed");
//!
//!     save_test_image(&image, "my_capture");
//! }
//! ```
//!
//! ## Linux/Wayland
//!
//! ```ignore
//! use screenshot_test_utils::wayland::{
//!     create_test_backend_with_store, print_test_environment,
//! };
//! use screenshot_core::util::key_store::KeyStore;
//! use std::sync::Arc;
//!
//! #[tokio::test]
//! async fn test_wayland_capture() {
//!     print_test_environment();
//!
//!     let key_store = Arc::new(KeyStore::new());
//!     let backend = create_test_backend_with_store(key_store);
//!     // ... run tests ...
//! }
//! ```
//!
//! ## Cross-Platform Timing
//!
//! ```ignore
//! use screenshot_test_utils::timing::{measure_sync, assert_duration_below};
//! use std::time::Duration;
//!
//! let (result, duration) = measure_sync("expensive_op", || expensive_computation());
//! assert_duration_below(duration, Duration::from_secs(5), "expensive_op");
//! ```
//!
//! # MCP Server Testing
//!
//! For MCP protocol-level testing, use the `mcp_harness` module in
//! `screenshot-mcp-server/tests/common/`. It's kept separate because it
//! depends on the server crate itself.
//!
//! ```ignore
//! // In screenshot-mcp-server tests:
//! mod common;
//! use common::mcp_harness::MockScreenshotServer;
//!
//! #[tokio::test]
//! async fn test_mcp_tool() {
//!     let server = MockScreenshotServer::new();
//!     // ... test MCP protocol ...
//! }
//! ```

// Cross-platform timing utilities (always available)
pub mod timing;

// Platform-specific modules
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod wayland;
