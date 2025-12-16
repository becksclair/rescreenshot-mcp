//! Linux/Wayland test utilities
//!
//! This module provides test harness utilities for Wayland integration tests.
//!
//! # Key Features
//!
//! - Backend setup/teardown with KeyStore
//! - Environment validation and printing
//! - Token management helpers (setup, cleanup, verification)
//! - Duration assertion helpers
//!
//! # Example
//!
//! ```ignore
//! use screenshot_test_utils::wayland::{
//!     create_test_backend_with_store, print_test_environment,
//!     setup_test_token, cleanup_test_tokens,
//! };
//! use screenshot_core::util::key_store::KeyStore;
//! use std::sync::Arc;
//!
//! #[tokio::test]
//! async fn test_wayland_capture() {
//!     print_test_environment();
//!
//!     let key_store = Arc::new(KeyStore::new());
//!     let backend = create_test_backend_with_store(key_store.clone());
//!
//!     // Test with stored token
//!     setup_test_token(&key_store, "test_source", "test_token").unwrap();
//!     // ... run tests ...
//!     cleanup_test_tokens(&key_store, &["test_source"]);
//! }
//! ```

#![allow(dead_code)]

#[cfg(target_os = "linux")]
use std::sync::Arc;

// Re-export timing utilities from the timing module
pub use crate::timing::{
    PerformanceThresholds, assert_duration_above, assert_duration_below, measure_operation,
    print_timing_result,
};

#[cfg(target_os = "linux")]
use screenshot_core::{
    capture::wayland_backend::WaylandBackend, error::CaptureError, model::CaptureOptions,
    util::key_store::KeyStore,
};

/// Creates a WaylandBackend with a shared KeyStore for testing token operations
#[cfg(target_os = "linux")]
pub fn create_test_backend_with_store(key_store: Arc<KeyStore>) -> WaylandBackend {
    WaylandBackend::new(key_store)
}

/// Prints test environment information
///
/// Displays Wayland-related environment variables for debugging test failures.
#[cfg(target_os = "linux")]
pub fn print_test_environment() {
    eprintln!("=== Wayland Test Environment ===");
    eprintln!(
        "WAYLAND_DISPLAY: {}",
        std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "NOT SET".to_string())
    );
    eprintln!(
        "XDG_SESSION_TYPE: {}",
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "NOT SET".to_string())
    );
    eprintln!(
        "DBUS_SESSION_BUS_ADDRESS: {}",
        if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok() {
            "SET"
        } else {
            "NOT SET"
        }
    );
    eprintln!("================================\n");
}

/// Cleans up test tokens from KeyStore
///
/// Removes tokens created during tests. Logs warnings for any cleanup failures
/// but doesn't panic, since cleanup failures shouldn't fail tests.
#[cfg(target_os = "linux")]
pub fn cleanup_test_tokens(key_store: &KeyStore, source_ids: &[&str]) {
    for source_id in source_ids {
        if let Err(e) = key_store.delete_token(source_id) {
            eprintln!("Warning: Failed to cleanup token '{}': {}", source_id, e);
        }
    }
}

/// Stores a test token and returns cleanup function
#[cfg(target_os = "linux")]
pub fn setup_test_token(
    key_store: &KeyStore,
    source_id: &str,
    token: &str,
) -> Result<(), CaptureError> {
    key_store.store_token(source_id, token)
}

/// Verifies token exists in KeyStore
///
/// # Panics
///
/// Panics if the token doesn't exist or if checking fails.
#[cfg(target_os = "linux")]
pub fn assert_token_exists(key_store: &KeyStore, source_id: &str) {
    assert!(
        key_store
            .has_token(source_id)
            .expect("Failed to check token"),
        "Token '{}' should exist in KeyStore",
        source_id
    );
}

/// Creates default capture options for testing
#[cfg(target_os = "linux")]
pub fn default_test_capture_options() -> CaptureOptions {
    CaptureOptions::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_assert_duration_below_success() {
        assert_duration_below(Duration::from_millis(900), Duration::from_secs(1), "test operation");
    }

    #[test]
    fn test_assert_duration_above_success() {
        assert_duration_above(
            Duration::from_millis(1100),
            Duration::from_secs(1),
            "test operation",
        );
    }

    // Note: #[should_panic] tests removed due to Windows test harness issues
    // with cross-crate panic propagation. The panic behavior is already
    // tested in screenshot-core's wayland_harness module tests.
}
