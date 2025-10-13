//! Mock capture backend for testing
//!
//! This module provides a `MockBackend` implementation of the [`CaptureFacade`]
//! trait for testing and development purposes. The mock backend generates
//! synthetic test images and simulates window enumeration without requiring
//! access to a real windowing system.
//!
//! # Phase 8 Implementation (Not Yet Complete)
//!
//! The `MockBackend` will be fully implemented in M1 Phase 8 with the following
//! features:
//!
//! - Configurable delay simulation for async operations
//! - Error injection for testing error handling paths
//! - Mock window list with predefined entries (Firefox, VSCode, Terminal)
//! - Window selector resolution with fuzzy matching
//! - Test pattern image generation at specified dimensions
//! - Full capabilities support
//!
//! # Examples (Preview)
//!
//! ```rust,ignore
//! use screenshot_mcp::capture::{CaptureFacade, mock::MockBackend};
//! use screenshot_mcp::model::{CaptureOptions, WindowSelector};
//!
//! #[tokio::main]
//! async fn main() {
//!     // Create mock backend
//!     let backend = MockBackend::new();
//!
//!     // List mock windows
//!     let windows = backend.list_windows().await.unwrap();
//!     println!("Found {} windows", windows.len());
//!
//!     // Capture a window
//!     let selector = WindowSelector::by_title("Firefox");
//!     let handle = backend.resolve_target(&selector).await.unwrap();
//!     let opts = CaptureOptions::default();
//!     let image = backend.capture_window(handle, &opts).await.unwrap();
//!     println!("Captured {}x{} image", image.dimensions().0, image.dimensions().1);
//! }
//! ```

// TODO: Implement MockBackend in Phase 8
