//! Shared test utilities for integration and performance tests

#[cfg(feature = "linux-wayland")]
pub mod wayland_harness;

#[cfg(feature = "windows-backend")]
pub mod windows_helpers;
