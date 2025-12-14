//! Wayland integration test harness with timing and assertion utilities
//!
//! This module provides shared utilities for Wayland integration tests:
//! - Backend setup/teardown
//! - Timing measurement for performance tests (reexported from
//!   [`screenshot_core::perf`])
//! - Environment validation
//! - Assertion helpers

#![allow(dead_code)]

#[cfg(target_os = "linux")]
use std::sync::Arc;
use std::time::Duration;

// Re-export performance utilities from the main library
// These are always available during testing since perf module has #[cfg(any(feature =
// "perf-tests", test))]
#[cfg(any(feature = "perf-tests", test))]
#[allow(unused_imports)]
pub use screenshot_core::perf::{PerformanceThresholds, measure_operation, print_timing_result};
#[cfg(target_os = "linux")]
use screenshot_core::{
    capture::wayland_backend::WaylandBackend, model::CaptureOptions, util::key_store::KeyStore,
};

/// Creates a WaylandBackend with a shared KeyStore for testing token operations
#[cfg(target_os = "linux")]
pub fn create_test_backend_with_store(key_store: Arc<KeyStore>) -> WaylandBackend {
    WaylandBackend::new(key_store)
}

/// Prints test environment information
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

/// Asserts that a duration is below a threshold
pub fn assert_duration_below(actual: Duration, threshold: Duration, operation: &str) {
    assert!(
        actual <= threshold,
        "{} took {:.3}s, expected <={:.3}s ({}ms over threshold)",
        operation,
        actual.as_secs_f64(),
        threshold.as_secs_f64(),
        (actual.as_millis() as i128) - (threshold.as_millis() as i128)
    );
}

/// Asserts that a duration is above a minimum (for sanity checks)
pub fn assert_duration_above(actual: Duration, minimum: Duration, operation: &str) {
    assert!(
        actual >= minimum,
        "{} took {:.3}s, expected >={:.3}s (suspiciously fast)",
        operation,
        actual.as_secs_f64(),
        minimum.as_secs_f64()
    );
}

/// Cleans up test tokens from KeyStore
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
) -> Result<(), screenshot_core::error::CaptureError> {
    key_store.store_token(source_id, token)
}

/// Verifies token exists in KeyStore
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

    // Tests for PerformanceThresholds, TimingResult, and timing functions
    // are now in src/perf/mod.rs

    #[test]
    fn test_assert_duration_below_success() {
        assert_duration_below(Duration::from_millis(900), Duration::from_secs(1), "test operation");
    }

    #[test]
    #[should_panic(expected = "took")]
    fn test_assert_duration_below_failure() {
        assert_duration_below(
            Duration::from_millis(1100),
            Duration::from_secs(1),
            "test operation",
        );
    }

    #[test]
    fn test_assert_duration_above_success() {
        assert_duration_above(
            Duration::from_millis(1100),
            Duration::from_secs(1),
            "test operation",
        );
    }

    #[test]
    #[should_panic(expected = "suspiciously fast")]
    fn test_assert_duration_above_failure() {
        assert_duration_above(Duration::from_millis(900), Duration::from_secs(1), "test operation");
    }
}
