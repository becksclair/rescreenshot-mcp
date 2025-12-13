//! Performance measurement utilities for screenshot-mcp
//!
//! This module provides shared utilities for measuring capture performance
//! across integration tests and CLI tools. Available when `perf-tests` feature
//! is enabled, or during testing.
//!
//! ## Key Components
//!
//! - [`PerformanceThresholds`]: M2 exit criteria thresholds
//! - [`TimingResult`]: Captured timing data for operations
//! - [`measure_operation`]: Async operation timing wrapper
//! - [`time_async`]: Simple async duration measurement
//!
//! ## Usage
//!
//! ```ignore
//! use screenshot_mcp::perf::*;
//!
//! let result = measure_operation(
//!     "my_operation",
//!     async { Ok::<_, String>("result") }
//! ).await?;
//!
//! print_timing_result(&result.1);
//! ```

use std::time::{Duration, Instant};

/// Performance thresholds for M2 exit criteria
///
/// These thresholds define the acceptable performance targets for Wayland
/// capture operations. All values are derived from M2 requirements.
#[derive(Debug, Clone, Copy)]
pub struct PerformanceThresholds {
    /// Prime consent flow duration (excluding user interaction time): target
    /// <5s
    pub prime_consent_max: Duration,

    /// Headless capture latency P95 (95th percentile): target <2s
    pub capture_latency_p95: Duration,

    /// Token rotation overhead: target <100ms
    pub token_rotation_max: Duration,

    /// Memory peak during capture: target <200MB
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

impl PerformanceThresholds {
    /// Creates a new set of thresholds with custom values
    pub fn new(
        prime_consent_max: Duration,
        capture_latency_p95: Duration,
        token_rotation_max: Duration,
        memory_peak_max_bytes: usize,
    ) -> Self {
        Self {
            prime_consent_max,
            capture_latency_p95,
            token_rotation_max,
            memory_peak_max_bytes,
        }
    }

    /// Checks if prime consent duration meets threshold
    pub fn check_prime_consent(&self, duration: Duration) -> bool {
        duration <= self.prime_consent_max
    }

    /// Checks if capture latency meets P95 threshold
    pub fn check_capture_latency(&self, duration: Duration) -> bool {
        duration <= self.capture_latency_p95
    }

    /// Checks if token rotation overhead meets threshold
    pub fn check_token_rotation(&self, duration: Duration) -> bool {
        duration <= self.token_rotation_max
    }

    /// Checks if memory peak meets threshold
    pub fn check_memory_peak(&self, bytes: usize) -> bool {
        bytes <= self.memory_peak_max_bytes
    }
}

/// Test timing result containing operation metadata and duration
///
/// Returned by [`measure_operation`] to provide structured timing data.
#[derive(Debug, Clone)]
pub struct TimingResult {
    /// Human-readable operation name
    pub operation: String,

    /// Elapsed duration of the operation
    pub duration: Duration,

    /// Whether the operation succeeded (true) or failed (false)
    pub success: bool,
}

impl TimingResult {
    /// Creates a new timing result
    pub fn new(operation: impl Into<String>, duration: Duration, success: bool) -> Self {
        Self {
            operation: operation.into(),
            duration,
            success,
        }
    }

    /// Returns duration in milliseconds
    pub fn duration_ms(&self) -> u128 {
        self.duration.as_millis()
    }

    /// Returns duration in seconds (floating point)
    pub fn duration_secs(&self) -> f64 {
        self.duration.as_secs_f64()
    }

    /// Returns a human-readable status string ("✓" or "✗")
    pub fn status_symbol(&self) -> &'static str {
        if self.success { "✓" } else { "✗" }
    }
}

/// Measures the duration of an async operation and returns result with timing
///
/// This function wraps an async operation and captures both its result and
/// execution duration. Useful for performance validation.
///
/// ## Example
///
/// ```ignore
/// use screenshot_mcp::perf::measure_operation;
///
/// let (value, timing) = measure_operation(
///     "database_query",
///     async { database.query("SELECT * FROM users").await }
/// ).await?;
///
/// println!("Query took {:.3}s", timing.duration_secs());
/// ```
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

/// Measures the duration of an async operation (non-failing variant)
///
/// Unlike [`measure_operation`], this function doesn't track success/failure.
/// Use when you need simple timing measurement for infallible operations.
///
/// ## Example
///
/// ```ignore
/// use screenshot_mcp::perf::time_async;
///
/// let (result, duration) = time_async(async {
///     expensive_computation()
/// }).await;
///
/// println!("Computation took {:.3}s", duration.as_secs_f64());
/// ```
pub async fn time_async<F, T>(operation: F) -> (T, Duration)
where
    F: std::future::Future<Output = T>,
{
    let start = Instant::now();
    let result = operation.await;
    let duration = start.elapsed();
    (result, duration)
}

/// Prints a timing result in a formatted way to stderr
///
/// Output format: `<status> <operation> took <duration_s>s (<duration_ms>ms)`
///
/// ## Example
///
/// ```ignore
/// use screenshot_mcp::perf::{TimingResult, print_timing_result};
/// use std::time::Duration;
///
/// let timing = TimingResult::new("capture", Duration::from_millis(1250), true);
/// print_timing_result(&timing);
/// // Outputs: ✓ capture took 1.250s (1250ms)
/// ```
pub fn print_timing_result(result: &TimingResult) {
    eprintln!(
        "{} {} took {:.3}s ({:.0}ms)",
        result.status_symbol(),
        result.operation,
        result.duration_secs(),
        result.duration_ms()
    );
}

/// Prints multiple timing results as a formatted table to stderr
///
/// ## Example
///
/// ```ignore
/// use screenshot_mcp::perf::{TimingResult, print_timing_summary};
/// use std::time::Duration;
///
/// let results = vec![
///     TimingResult::new("prime_consent", Duration::from_secs(3), true),
///     TimingResult::new("capture", Duration::from_millis(1500), true),
/// ];
///
/// print_timing_summary(&results);
/// // Outputs:
/// // === Timing Summary ===
/// // ✓ prime_consent took 3.000s (3000ms)
/// // ✓ capture took 1.500s (1500ms)
/// // ======================
/// ```
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
    fn test_performance_thresholds_checks() {
        let thresholds = PerformanceThresholds::default();

        // Prime consent
        assert!(thresholds.check_prime_consent(Duration::from_secs(4)));
        assert!(!thresholds.check_prime_consent(Duration::from_secs(6)));

        // Capture latency
        assert!(thresholds.check_capture_latency(Duration::from_millis(1500)));
        assert!(!thresholds.check_capture_latency(Duration::from_millis(2500)));

        // Token rotation
        assert!(thresholds.check_token_rotation(Duration::from_millis(50)));
        assert!(!thresholds.check_token_rotation(Duration::from_millis(150)));

        // Memory peak
        assert!(thresholds.check_memory_peak(150 * 1024 * 1024));
        assert!(!thresholds.check_memory_peak(250 * 1024 * 1024));
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
        assert_eq!(result.status_symbol(), "✓");
    }

    #[test]
    fn test_timing_result_status_symbols() {
        let success = TimingResult::new("success", Duration::from_secs(1), true);
        assert_eq!(success.status_symbol(), "✓");

        let failure = TimingResult::new("failure", Duration::from_secs(1), false);
        assert_eq!(failure.status_symbol(), "✗");
    }

    #[tokio::test]
    async fn test_measure_operation_success() {
        let (value, timing) = measure_operation("test_op", async { Ok::<_, String>(42) })
            .await
            .expect("Operation should succeed");

        assert_eq!(value, 42);
        assert_eq!(timing.operation, "test_op");
        assert!(timing.success);
        assert!(timing.duration.as_nanos() > 0);
    }

    #[tokio::test]
    async fn test_measure_operation_failure() {
        let result = measure_operation("failing_op", async { Err::<i32, _>("error") }).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_time_async() {
        let (value, duration) = time_async(async { 42 }).await;

        assert_eq!(value, 42);
        assert!(duration.as_nanos() > 0);
    }
}
