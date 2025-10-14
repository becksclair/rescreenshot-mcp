//! Wayland integration test harness with timing and assertion utilities
//!
//! This module provides shared utilities for Wayland integration tests:
//! - Backend setup/teardown
//! - Timing measurement for performance tests
//! - Environment validation
//! - Assertion helpers

#[cfg(feature = "linux-wayland")]
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(feature = "linux-wayland")]
use screenshot_mcp::{
    capture::{wayland_backend::WaylandBackend, CaptureFacade},
    model::{CaptureOptions, SourceType},
    util::key_store::KeyStore,
};

/// Performance thresholds for M2 exit criteria
pub struct PerformanceThresholds {
    /// Prime consent flow (excluding user interaction): <5s
    pub prime_consent_max:    Duration,
    /// Headless capture latency P95: <2s
    pub capture_latency_p95:  Duration,
    /// Token rotation overhead: <100ms
    pub token_rotation_max:   Duration,
    /// Memory peak during capture: <200MB
    pub memory_peak_max_bytes: usize,
}

impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            prime_consent_max:     Duration::from_secs(5),
            capture_latency_p95:   Duration::from_secs(2),
            token_rotation_max:    Duration::from_millis(100),
            memory_peak_max_bytes: 200 * 1024 * 1024, // 200MB
        }
    }
}

/// Test timing result
#[derive(Debug, Clone)]
pub struct TimingResult {
    pub operation: String,
    pub duration:  Duration,
    pub success:   bool,
}

impl TimingResult {
    pub fn duration_ms(&self) -> u128 {
        self.duration.as_millis()
    }

    pub fn duration_secs(&self) -> f64 {
        self.duration.as_secs_f64()
    }
}

/// Creates a WaylandBackend instance for testing
#[cfg(feature = "linux-wayland")]
pub fn create_test_backend() -> WaylandBackend {
    let key_store = Arc::new(KeyStore::new());
    WaylandBackend::new(key_store)
}

/// Creates a WaylandBackend with a shared KeyStore for testing token operations
#[cfg(feature = "linux-wayland")]
pub fn create_test_backend_with_store(key_store: Arc<KeyStore>) -> WaylandBackend {
    WaylandBackend::new(key_store)
}

/// Measures the duration of an async operation
#[cfg(feature = "linux-wayland")]
pub async fn measure_operation<F, T, E>(
    operation_name: &str,
    operation: F,
) -> Result<(T, TimingResult), E>
where
    F: std::future::Future<Output = Result<T, E>>,
{
    let start = Instant::now();
    let result = operation.await;
    let duration = start.elapsed();

    let timing = TimingResult {
        operation: operation_name.to_string(),
        duration,
        success: result.is_ok(),
    };

    result.map(|value| (value, timing))
}

/// Measures the duration of an operation and returns result with timing
#[cfg(feature = "linux-wayland")]
pub async fn time_async<F, T>(operation: F) -> (T, Duration)
where
    F: std::future::Future<Output = T>,
{
    let start = Instant::now();
    let result = operation.await;
    let duration = start.elapsed();
    (result, duration)
}

/// Checks if Wayland environment is available for testing
#[cfg(feature = "linux-wayland")]
pub fn is_wayland_available() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

/// Checks if XDG Desktop Portal is likely available
#[cfg(feature = "linux-wayland")]
pub fn is_portal_available() -> bool {
    // Basic check: DBUS_SESSION_BUS_ADDRESS should be set
    std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok()
}

/// Prints test environment information
#[cfg(feature = "linux-wayland")]
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
pub fn assert_duration_below(
    actual: Duration,
    threshold: Duration,
    operation: &str,
) {
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
pub fn assert_duration_above(
    actual: Duration,
    minimum: Duration,
    operation: &str,
) {
    assert!(
        actual >= minimum,
        "{} took {:.3}s, expected >={:.3}s (suspiciously fast)",
        operation,
        actual.as_secs_f64(),
        minimum.as_secs_f64()
    );
}

/// Cleans up test tokens from KeyStore
#[cfg(feature = "linux-wayland")]
pub fn cleanup_test_tokens(key_store: &KeyStore, source_ids: &[&str]) {
    for source_id in source_ids {
        if let Err(e) = key_store.delete_token(source_id) {
            eprintln!("Warning: Failed to cleanup token '{}': {}", source_id, e);
        }
    }
}

/// Stores a test token and returns cleanup function
#[cfg(feature = "linux-wayland")]
pub fn setup_test_token(
    key_store: &KeyStore,
    source_id: &str,
    token: &str,
) -> Result<(), screenshot_mcp::error::CaptureError> {
    key_store.store_token(source_id, token)
}

/// Verifies token exists in KeyStore
#[cfg(feature = "linux-wayland")]
pub fn assert_token_exists(key_store: &KeyStore, source_id: &str) {
    assert!(
        key_store.has_token(source_id).expect("Failed to check token"),
        "Token '{}' should exist in KeyStore",
        source_id
    );
}

/// Verifies token does not exist in KeyStore
#[cfg(feature = "linux-wayland")]
pub fn assert_token_not_exists(key_store: &KeyStore, source_id: &str) {
    assert!(
        !key_store.has_token(source_id).expect("Failed to check token"),
        "Token '{}' should NOT exist in KeyStore",
        source_id
    );
}

/// Creates default capture options for testing
#[cfg(feature = "linux-wayland")]
pub fn default_test_capture_options() -> CaptureOptions {
    CaptureOptions::default()
}

/// Prints timing result in a formatted way
pub fn print_timing_result(result: &TimingResult) {
    let status = if result.success { "✓" } else { "✗" };
    eprintln!(
        "{} {} took {:.3}s ({:.0}ms)",
        status,
        result.operation,
        result.duration_secs(),
        result.duration_ms()
    );
}

/// Prints multiple timing results as a table
pub fn print_timing_summary(results: &[TimingResult]) {
    eprintln!("\n=== Timing Summary ===");
    for result in results {
        print_timing_result(result);
    }
    eprintln!("======================\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_thresholds_defaults() {
        let thresholds = PerformanceThresholds::default();
        assert_eq!(thresholds.prime_consent_max, Duration::from_secs(5));
        assert_eq!(thresholds.capture_latency_p95, Duration::from_secs(2));
        assert_eq!(thresholds.token_rotation_max, Duration::from_millis(100));
        assert_eq!(thresholds.memory_peak_max_bytes, 200 * 1024 * 1024);
    }

    #[test]
    fn test_timing_result_conversions() {
        let result = TimingResult {
            operation: "test".to_string(),
            duration:  Duration::from_millis(1500),
            success:   true,
        };

        assert_eq!(result.duration_ms(), 1500);
        assert!((result.duration_secs() - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_assert_duration_below_success() {
        assert_duration_below(
            Duration::from_millis(900),
            Duration::from_secs(1),
            "test operation",
        );
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
        assert_duration_above(
            Duration::from_millis(900),
            Duration::from_secs(1),
            "test operation",
        );
    }
}
