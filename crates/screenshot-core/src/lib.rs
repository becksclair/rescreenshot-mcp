//! screenshot-core: Cross-platform screenshot capture library
//!
//! This library provides core screenshot capture functionality across different
//! platforms (Linux Wayland/X11, Windows, macOS). It includes platform backends,
//! window matching, image processing, and error handling.

#[cfg(feature = "image-processing")]
pub mod capture;
pub mod error;
pub mod model;
pub mod perf;
pub mod util;
