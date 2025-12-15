//! Cross-platform timing and performance measurement utilities
//!
//! This module provides shared timing utilities for integration tests across
//! all platforms. It re-exports performance utilities from `screenshot-core`
//! and adds simple timing helpers.
//!
//! # Re-exports from screenshot-core
//!
//! - [`PerformanceThresholds`]: M2 exit criteria thresholds for Wayland
//! - [`TimingResult`]: Structured timing data for operations
//! - [`measure_operation`]: Async operation timing wrapper
//! - [`time_async`]: Simple async duration measurement
//! - [`print_timing_result`]: Formatted timing output
//! - [`print_timing_summary`]: Multi-result timing table
//!
//! # Additional Utilities
//!
//! - [`measure_sync`]: Simple synchronous timing wrapper
//! - [`assert_duration_below`]: Assert duration is under threshold
//! - [`assert_duration_above`]: Assert duration exceeds minimum

use std::time::{Duration, Instant};

// Re-export performance utilities from screenshot-core
pub use screenshot_core::perf::{
    PerformanceThresholds, TimingResult, measure_operation, print_timing_result,
    print_timing_summary, time_async,
};

/// Measure the duration of a synchronous operation
///
/// Simple wrapper for timing sync operations. Returns the result and elapsed duration.
///
/// # Example
///
/// ```ignore
/// use screenshot_test_utils::timing::measure_sync;
///
/// let (result, duration) = measure_sync("compute", || expensive_computation());
/// println!("Computation took {:.2}ms", duration.as_secs_f64() * 1000.0);
/// ```
pub fn measure_sync<F, T>(name: &str, f: F) -> (T, Duration)
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();
    println!("[TIMING] {}: {:.2}ms", name, elapsed.as_secs_f64() * 1000.0);
    (result, elapsed)
}

/// Asserts that a duration is below a threshold
///
/// Panics with a descriptive message if the duration exceeds the threshold.
///
/// # Example
///
/// ```
/// use screenshot_test_utils::timing::assert_duration_below;
/// use std::time::Duration;
///
/// assert_duration_below(Duration::from_millis(500), Duration::from_secs(1), "capture");
/// ```
///
/// # Panics
///
/// Panics if `actual > threshold` with a message showing the excess time.
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
///
/// Useful for detecting suspiciously fast operations that may indicate
/// mocking, caching, or test infrastructure issues.
///
/// # Example
///
/// ```
/// use screenshot_test_utils::timing::assert_duration_above;
/// use std::time::Duration;
///
/// // Verify operation actually ran (wasn't cached/mocked)
/// assert_duration_above(Duration::from_millis(100), Duration::from_millis(10), "real_capture");
/// ```
///
/// # Panics
///
/// Panics if `actual < minimum` with a message noting the suspiciously fast time.
pub fn assert_duration_above(actual: Duration, minimum: Duration, operation: &str) {
    assert!(
        actual >= minimum,
        "{} took {:.3}s, expected >={:.3}s (suspiciously fast)",
        operation,
        actual.as_secs_f64(),
        minimum.as_secs_f64()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_measure_sync() {
        let (result, duration) = measure_sync("test", || 42);
        assert_eq!(result, 42);
        assert!(duration.as_nanos() > 0);
    }

    #[test]
    fn test_assert_duration_below_success() {
        assert_duration_below(Duration::from_millis(500), Duration::from_secs(1), "test");
    }

    #[test]
    fn test_assert_duration_above_success() {
        assert_duration_above(Duration::from_millis(1500), Duration::from_secs(1), "test");
    }

    // Note: #[should_panic] tests removed due to Windows test harness issues
    // with cross-crate panic propagation. The panic behavior is tested
    // in screenshot-core's wayland_harness module tests.
}
