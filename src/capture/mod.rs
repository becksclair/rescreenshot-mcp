//! Capture backend traits and implementations
//!
//! This module provides the core abstractions for screenshot capture across
//! different platforms. It includes:
//!
//! - `ImageBuffer`: A wrapper around `image::DynamicImage` with transformation
//!   methods for scaling, cropping, and format conversion
//! - `CaptureFacade`: Trait defining the interface for capture backends (to be
//!   implemented in future phases)
//! - Backend implementations for Wayland, X11, Windows, and macOS (to be
//!   implemented in future phases)

pub mod image_buffer;

pub use image_buffer::ImageBuffer;
