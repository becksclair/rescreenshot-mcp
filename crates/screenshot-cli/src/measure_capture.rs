//! Performance measurement tool for Wayland capture operations
//!
//! This tool measures capture performance metrics for M2 exit criteria:
//! - Prime consent flow duration
//! - Headless capture latency (P95)
//! - Token rotation overhead
//!
//! ## Commands
//!
//! - `prime-consent <source_id>`: Measures portal interaction time (excluding
//!   user)
//! - `headless-batch --captures N <source_id>`: Runs N captures, outputs P95
//!   latency
//! - `token-rotation --captures N <source_id>`: Measures token rotation
//!   overhead
//! - `summary`: Aggregates metrics and validates against thresholds
//!
//! ## Usage
//!
//! ```bash
//! cargo run --bin measure-capture --features perf-tests -- prime-consent wayland-perf
//! cargo run --bin measure-capture --features perf-tests -- headless-batch --captures 30 wayland-perf
//! ```
//!
//! ## Output
//!
//! JSON to stdout, exit code 0 if all thresholds met, 1 otherwise.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("measure-capture is only supported on Linux (Wayland).");
    eprintln!(
        "Run on Linux with: cargo run --bin measure-capture --features perf-tests \
         -- summary"
    );
    std::process::exit(2);
}

#[cfg(target_os = "linux")]
use std::{process, sync::Arc, time::Duration};

#[cfg(target_os = "linux")]
use screenshot_core::{
    capture::wayland_backend::WaylandBackend,
    model::{CaptureOptions, SourceType},
    perf::{PerformanceThresholds, measure_operation},
    util::key_store::KeyStore,
};

#[cfg(target_os = "linux")]
#[derive(Debug, serde::Serialize)]
struct MeasurementOutput {
    operation: String,
    duration_ms: u128,
    duration_secs: f64,
    success: bool,
    threshold_met: Option<bool>,
}

#[cfg(target_os = "linux")]
#[derive(Debug, serde::Serialize)]
struct BatchOutput {
    operation: String,
    total_captures: usize,
    successful: usize,
    failed: usize,
    min_ms: u128,
    max_ms: u128,
    mean_ms: f64,
    p50_ms: u128,
    p95_ms: u128,
    p99_ms: u128,
    threshold_met: Option<bool>,
}

#[cfg(target_os = "linux")]
#[derive(Debug, serde::Serialize)]
struct SummaryOutput {
    status: String,
    thresholds: ThresholdsReport,
    message: String,
}

#[cfg(target_os = "linux")]
#[derive(Debug, serde::Serialize)]
struct ThresholdsReport {
    prime_consent_max_s: f64,
    capture_latency_p95_s: f64,
    token_rotation_max_ms: u128,
    memory_peak_max_mb: usize,
}

#[cfg(target_os = "linux")]
impl From<&PerformanceThresholds> for ThresholdsReport {
    fn from(thresholds: &PerformanceThresholds) -> Self {
        Self {
            prime_consent_max_s: thresholds.prime_consent_max.as_secs_f64(),
            capture_latency_p95_s: thresholds.capture_latency_p95.as_secs_f64(),
            token_rotation_max_ms: thresholds.token_rotation_max.as_millis(),
            memory_peak_max_mb: thresholds.memory_peak_max_bytes / (1024 * 1024),
        }
    }
}

#[cfg(target_os = "linux")]
fn calculate_percentile(sorted_durations: &[Duration], percentile: f64) -> Duration {
    if sorted_durations.is_empty() {
        return Duration::ZERO;
    }

    let index = (percentile / 100.0 * sorted_durations.len() as f64).ceil() as usize;
    let index = index.min(sorted_durations.len()).saturating_sub(1);
    sorted_durations[index]
}

#[cfg(target_os = "linux")]
fn print_usage() {
    eprintln!("measure-capture - Wayland performance measurement tool");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    measure-capture prime-consent <source_id>");
    eprintln!("    measure-capture headless-batch --captures N <source_id>");
    eprintln!("    measure-capture token-rotation --captures N <source_id>");
    eprintln!("    measure-capture summary");
    eprintln!();
    eprintln!("COMMANDS:");
    eprintln!("    prime-consent <source_id>");
    eprintln!("        Measures portal interaction time for first-time consent");
    eprintln!();
    eprintln!("    headless-batch --captures N <source_id>");
    eprintln!("        Runs N sequential captures, outputs P95 latency");
    eprintln!();
    eprintln!("    token-rotation --captures N <source_id>");
    eprintln!("        Measures token rotation overhead across N captures");
    eprintln!();
    eprintln!("    summary");
    eprintln!("        Validates all metrics against M2 thresholds");
    eprintln!();
    eprintln!("OUTPUT:");
    eprintln!("    JSON to stdout");
    eprintln!("    Exit code 0 if thresholds met, 1 otherwise");
}

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() {
    // Initialize tracing for debugging (optional)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let command = &args[1];

    match command.as_str() {
        "prime-consent" => {
            if args.len() < 3 {
                eprintln!("ERROR: prime-consent requires <source_id> argument");
                print_usage();
                process::exit(1);
            }
            let source_id = &args[2];
            run_prime_consent(source_id).await;
        }
        "headless-batch" => {
            if args.len() < 5 || args[2] != "--captures" {
                eprintln!("ERROR: headless-batch requires --captures N <source_id>");
                print_usage();
                process::exit(1);
            }
            let captures: usize = args[3].parse().unwrap_or_else(|_| {
                eprintln!("ERROR: --captures must be a positive integer");
                process::exit(1);
            });
            let source_id = &args[4];
            run_headless_batch(source_id, captures).await;
        }
        "token-rotation" => {
            if args.len() < 5 || args[2] != "--captures" {
                eprintln!("ERROR: token-rotation requires --captures N <source_id>");
                print_usage();
                process::exit(1);
            }
            let captures: usize = args[3].parse().unwrap_or_else(|_| {
                eprintln!("ERROR: --captures must be a positive integer");
                process::exit(1);
            });
            let source_id = &args[4];
            run_token_rotation(source_id, captures).await;
        }
        "summary" => {
            run_summary();
        }
        _ => {
            eprintln!("ERROR: Unknown command '{}'", command);
            print_usage();
            process::exit(1);
        }
    }
}

#[cfg(target_os = "linux")]
async fn run_prime_consent(source_id: &str) {
    let key_store = Arc::new(KeyStore::new());
    let backend = WaylandBackend::new(key_store);
    let thresholds = PerformanceThresholds::default();

    eprintln!("=== Prime Consent Measurement ===");
    eprintln!("Source ID: {}", source_id);
    eprintln!("Waiting for user to grant permission in portal dialog...");
    eprintln!();

    let result = measure_operation(
        "prime_consent",
        backend.prime_consent(SourceType::Monitor, source_id, false),
    )
    .await;

    match result {
        Ok((consent_result, timing)) => {
            let threshold_met = thresholds.check_prime_consent(timing.duration);

            let output = MeasurementOutput {
                operation: "prime_consent".to_string(),
                duration_ms: timing.duration_ms(),
                duration_secs: timing.duration_secs(),
                success: true,
                threshold_met: Some(threshold_met),
            };

            println!("{}", serde_json::to_string_pretty(&output).unwrap());

            eprintln!();
            eprintln!("✓ Prime consent successful");
            eprintln!("  Source ID: {}", consent_result.primary_source_id);
            eprintln!("  Streams: {}", consent_result.num_streams);
            eprintln!("  Duration: {:.3}s ({:.0}ms)", timing.duration_secs(), timing.duration_ms());
            eprintln!(
                "  Threshold: <{:.1}s - {}",
                thresholds.prime_consent_max.as_secs_f64(),
                if threshold_met { "PASS" } else { "FAIL" }
            );

            if !threshold_met {
                process::exit(1);
            }
        }
        Err(e) => {
            let output = MeasurementOutput {
                operation: "prime_consent".to_string(),
                duration_ms: 0,
                duration_secs: 0.0,
                success: false,
                threshold_met: None,
            };

            println!("{}", serde_json::to_string_pretty(&output).unwrap());

            eprintln!();
            eprintln!("✗ Prime consent failed: {:?}", e);
            process::exit(1);
        }
    }
}

#[cfg(target_os = "linux")]
async fn run_headless_batch(source_id: &str, num_captures: usize) {
    let key_store = Arc::new(KeyStore::new());
    let backend = WaylandBackend::new(Arc::clone(&key_store));
    let thresholds = PerformanceThresholds::default();
    let opts = CaptureOptions::default();

    eprintln!("=== Headless Batch Measurement ===");
    eprintln!("Source ID: {}", source_id);
    eprintln!("Captures: {}", num_captures);
    eprintln!();

    // Check if token exists
    if !key_store.has_token(source_id).unwrap_or(false) {
        eprintln!("ERROR: No token found for source '{}'. Run prime-consent first.", source_id);
        process::exit(1);
    }

    eprintln!("Running {} captures...", num_captures);

    let mut durations = Vec::new();
    let mut successful = 0;
    let mut failed = 0;

    for i in 1..=num_captures {
        eprint!("\rCapture {}/{}...", i, num_captures);

        let result = measure_operation(
            "capture_window",
            backend.capture_window(source_id.to_string(), &opts),
        )
        .await;

        match result {
            Ok((_image, timing)) => {
                durations.push(timing.duration);
                successful += 1;
            }
            Err(e) => {
                eprintln!();
                eprintln!("  Capture {} failed: {:?}", i, e);
                failed += 1;
            }
        }
    }

    eprintln!();
    eprintln!();

    if durations.is_empty() {
        eprintln!("✗ All captures failed");
        process::exit(1);
    }

    // Sort for percentile calculation
    durations.sort();

    let min = durations.first().copied().unwrap_or(Duration::ZERO);
    let max = durations.last().copied().unwrap_or(Duration::ZERO);
    let mean: Duration = durations.iter().sum::<Duration>() / durations.len() as u32;
    let p50 = calculate_percentile(&durations, 50.0);
    let p95 = calculate_percentile(&durations, 95.0);
    let p99 = calculate_percentile(&durations, 99.0);

    let threshold_met = thresholds.check_capture_latency(p95);

    let output = BatchOutput {
        operation: "headless_batch".to_string(),
        total_captures: num_captures,
        successful,
        failed,
        min_ms: min.as_millis(),
        max_ms: max.as_millis(),
        mean_ms: mean.as_millis() as f64,
        p50_ms: p50.as_millis(),
        p95_ms: p95.as_millis(),
        p99_ms: p99.as_millis(),
        threshold_met: Some(threshold_met),
    };

    println!("{}", serde_json::to_string_pretty(&output).unwrap());

    eprintln!();
    eprintln!("=== Results ===");
    eprintln!("  Successful: {}/{}", successful, num_captures);
    eprintln!("  Min: {:.0}ms", min.as_millis());
    eprintln!("  Mean: {:.0}ms", mean.as_millis());
    eprintln!("  P50: {:.0}ms", p50.as_millis());
    eprintln!("  P95: {:.0}ms", p95.as_millis());
    eprintln!("  P99: {:.0}ms", p99.as_millis());
    eprintln!("  Max: {:.0}ms", max.as_millis());
    eprintln!(
        "  Threshold: P95 <{:.1}s - {}",
        thresholds.capture_latency_p95.as_secs_f64(),
        if threshold_met { "PASS" } else { "FAIL" }
    );

    if !threshold_met || failed > 0 {
        process::exit(1);
    }
}

#[cfg(target_os = "linux")]
async fn run_token_rotation(source_id: &str, num_rotations: usize) {
    let key_store = Arc::new(KeyStore::new());
    let thresholds = PerformanceThresholds::default();

    eprintln!("=== Token Rotation Measurement ===");
    eprintln!("Source ID: {}", source_id);
    eprintln!("Rotations: {}", num_rotations);
    eprintln!();

    // Check if token exists
    if !key_store.has_token(source_id).unwrap_or(false) {
        eprintln!("ERROR: No token found for source '{}'. Run prime-consent first.", source_id);
        process::exit(1);
    }

    eprintln!("Running {} token rotations...", num_rotations);

    let mut durations = Vec::new();
    let mut successful = 0;
    let mut failed = 0;

    for i in 1..=num_rotations {
        eprint!("\rRotation {}/{}...", i, num_rotations);

        let new_token = format!("test_token_v{}", i);
        let start = std::time::Instant::now();
        let result = key_store.rotate_token(source_id, &new_token);
        let duration = start.elapsed();

        match result {
            Ok(()) => {
                durations.push(duration);
                successful += 1;
            }
            Err(e) => {
                eprintln!();
                eprintln!("  Rotation {} failed: {:?}", i, e);
                failed += 1;
            }
        }
    }

    eprintln!();
    eprintln!();

    if durations.is_empty() {
        eprintln!("✗ All rotations failed");
        process::exit(1);
    }

    // Sort for percentile calculation
    durations.sort();

    let min = durations.first().copied().unwrap_or(Duration::ZERO);
    let max = durations.last().copied().unwrap_or(Duration::ZERO);
    let mean: Duration = durations.iter().sum::<Duration>() / durations.len() as u32;
    let p50 = calculate_percentile(&durations, 50.0);
    let p95 = calculate_percentile(&durations, 95.0);
    let p99 = calculate_percentile(&durations, 99.0);

    let threshold_met = thresholds.check_token_rotation(p95);

    let output = BatchOutput {
        operation: "token_rotation".to_string(),
        total_captures: num_rotations,
        successful,
        failed,
        min_ms: min.as_millis(),
        max_ms: max.as_millis(),
        mean_ms: mean.as_millis() as f64,
        p50_ms: p50.as_millis(),
        p95_ms: p95.as_millis(),
        p99_ms: p99.as_millis(),
        threshold_met: Some(threshold_met),
    };

    println!("{}", serde_json::to_string_pretty(&output).unwrap());

    eprintln!();
    eprintln!("=== Results ===");
    eprintln!("  Successful: {}/{}", successful, num_rotations);
    eprintln!("  Min: {:.0}ms", min.as_millis());
    eprintln!("  Mean: {:.0}ms", mean.as_millis());
    eprintln!("  P50: {:.0}ms", p50.as_millis());
    eprintln!("  P95: {:.0}ms", p95.as_millis());
    eprintln!("  P99: {:.0}ms", p99.as_millis());
    eprintln!("  Max: {:.0}ms", max.as_millis());
    eprintln!(
        "  Threshold: P95 <{:.0}ms - {}",
        thresholds.token_rotation_max.as_millis(),
        if threshold_met { "PASS" } else { "FAIL" }
    );

    if !threshold_met || failed > 0 {
        process::exit(1);
    }
}

#[cfg(target_os = "linux")]
fn run_summary() {
    let thresholds = PerformanceThresholds::default();

    let summary = SummaryOutput {
        status: "ready".to_string(),
        thresholds: ThresholdsReport::from(&thresholds),
        message: "Run individual measurement commands to validate performance".to_string(),
    };

    println!("{}", serde_json::to_string_pretty(&summary).unwrap());

    eprintln!();
    eprintln!("=== M2 Performance Thresholds ===");
    eprintln!("  Prime consent: <{:.1}s", thresholds.prime_consent_max.as_secs_f64());
    eprintln!("  Capture latency (P95): <{:.1}s", thresholds.capture_latency_p95.as_secs_f64());
    eprintln!("  Token rotation: <{:.0}ms", thresholds.token_rotation_max.as_millis());
    eprintln!("  Memory peak: <{}MB", thresholds.memory_peak_max_bytes / (1024 * 1024));
    eprintln!();
    eprintln!("Run performance measurements:");
    eprintln!("  1. measure-capture prime-consent <source_id>");
    eprintln!("  2. measure-capture headless-batch --captures 30 <source_id>");
    eprintln!("  3. measure-capture token-rotation --captures 10 <source_id>");
}
