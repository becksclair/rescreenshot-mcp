//! Shared test utilities for integration and performance tests

#[cfg(target_os = "linux")]
pub mod wayland_harness;

#[cfg(target_os = "windows")]
pub mod windows_helpers;
