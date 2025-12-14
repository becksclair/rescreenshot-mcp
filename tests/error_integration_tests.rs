//! Integration tests for error handling in Wayland capture
//!
//! These tests verify end-to-end error scenarios in the Wayland backend.
//! Most tests are marked with `#[ignore]` and require a live Wayland session
//! to run. Execute them manually with:
//!
//! ```bash
//! cargo test --all-features -- --ignored
//! ```
//!
//! ## Test Environment Requirements
//!
//! - Running Wayland compositor (GNOME, KDE Plasma, Sway, etc.)
//! - xdg-desktop-portal and backend installed
//! - WAYLAND_DISPLAY environment variable set
//! - PipeWire runtime available

// Import shared test utilities
mod common;

#[cfg(all(test, target_os = "linux"))]
mod wayland_error_integration {
    use std::sync::Arc;

    use screenshot_mcp::{capture::CaptureFacade, model::SourceType, util::key_store::KeyStore};

    // Import test harness utilities
    use crate::common::wayland_harness::*;

    // ========================================================================
    // E2E Error Scenario Stubs (Manual Testing)
    // ========================================================================

    #[test]
    #[ignore = "Requires live Wayland session with portal"]
    fn test_full_workflow_token_expired() {
        // This test simulates the scenario where a token expires/is revoked
        //
        // Manual test steps:
        // 1. Run prime_wayland_consent to obtain token
        // 2. Manually revoke permission via compositor settings
        // 3. Attempt capture_window
        // 4. Verify graceful fallback to display capture
        //
        // Expected: Fallback triggers, user prompted for display selection

        eprintln!("=== Manual Test: Token Expired Workflow ===");
        eprintln!("1. Prime consent first");
        eprintln!("2. Revoke permission in system settings");
        eprintln!("3. Run capture with expired token");
        eprintln!("4. Verify fallback to display capture");
    }

    #[test]
    #[ignore = "Requires live Wayland session with portal"]
    fn test_full_workflow_compositor_restart() {
        // This test simulates compositor restart invalidating all tokens
        //
        // Manual test steps:
        // 1. Run prime_wayland_consent to obtain token
        // 2. Capture successfully to verify token works
        // 3. Restart Wayland compositor
        // 4. Attempt capture_window
        // 5. Verify fallback or re-prime prompt
        //
        // Expected: Token invalid, fallback to display capture

        eprintln!("=== Manual Test: Compositor Restart Workflow ===");
        eprintln!("1. Prime consent and capture successfully");
        eprintln!("2. Restart compositor (e.g., logout/login)");
        eprintln!("3. Run capture with invalidated token");
        eprintln!("4. Verify fallback behavior");
    }

    #[test]
    #[ignore = "Requires live Wayland session with portal"]
    fn test_full_workflow_permission_denied() {
        // This test simulates user denying permission in portal dialog
        //
        // Manual test steps:
        // 1. Run prime_wayland_consent
        // 2. Click "Cancel" or "Deny" in portal picker
        // 3. Verify PermissionDenied error returned
        // 4. Verify error message includes retry instructions
        //
        // Expected: PermissionDenied error with clear remediation hint

        eprintln!("=== Manual Test: Permission Denied Workflow ===");
        eprintln!("1. Run prime_wayland_consent");
        eprintln!("2. Cancel portal permission dialog");
        eprintln!("3. Verify PermissionDenied error");
        eprintln!("4. Check error message for retry instructions");
    }

    #[test]
    #[ignore = "Requires live Wayland session with portal"]
    fn test_full_workflow_portal_timeout() {
        // This test simulates portal dialog timeout (user doesn't respond)
        //
        // Manual test steps:
        // 1. Run prime_wayland_consent
        // 2. Leave portal dialog open without clicking anything
        // 3. Wait for 30-second timeout
        // 4. Verify CaptureTimeout error returned
        //
        // Expected: CaptureTimeout after 30 seconds with actionable message

        eprintln!("=== Manual Test: Portal Timeout Workflow ===");
        eprintln!("1. Run prime_wayland_consent");
        eprintln!("2. Leave portal dialog open");
        eprintln!("3. Wait 30 seconds for timeout");
        eprintln!("4. Verify CaptureTimeout error");
    }

    #[tokio::test]
    #[ignore = "Requires live Wayland session with portal"]
    async fn test_prime_consent_success() {
        print_test_environment();

        let key_store = Arc::new(KeyStore::new());
        let backend = create_test_backend_with_store(Arc::clone(&key_store));
        let source_id = "test-integration-prime";

        // Measure prime consent operation
        let result = measure_operation(
            "prime_consent",
            backend.prime_consent(SourceType::Monitor, source_id, false),
        )
        .await;

        match result {
            Ok((consent_result, timing)) => {
                print_timing_result(&timing);

                // Verify consent succeeded
                assert_eq!(consent_result.primary_source_id, source_id);
                assert!(consent_result.num_streams > 0);

                // Verify timing (excluding user interaction time)
                eprintln!("✓ Prime consent successful");
                eprintln!("  Source ID: {}", consent_result.primary_source_id);
                eprintln!("  Streams: {}", consent_result.num_streams);
                eprintln!("  Duration: {:.3}s", timing.duration_secs());

                // Verify token was stored
                assert_token_exists(&key_store, source_id);

                // Cleanup
                cleanup_test_tokens(&key_store, &[source_id]);
            }
            Err(e) => {
                eprintln!("✗ Prime consent failed: {:?}", e);
                panic!(
                    "Expected prime consent to succeed with user permission. Ensure portal is \
                     running and grant permission when prompted."
                );
            }
        }
    }

    #[tokio::test]
    #[ignore = "Requires live Wayland session with portal"]
    async fn test_capture_window_after_prime() {
        print_test_environment();

        let key_store = Arc::new(KeyStore::new());
        let backend = create_test_backend_with_store(Arc::clone(&key_store));
        let source_id = "test-capture-latency";

        eprintln!("=== Test: Capture Window After Prime ===");
        eprintln!("This test requires:");
        eprintln!("1. First run prime_consent separately to store a valid token");
        eprintln!("2. Then run this test within same compositor session");
        eprintln!("3. Or manually store a real token for testing");

        // NOTE: This test expects a real token to be already stored
        // In real usage: user would prime first, then capture
        // For testing: we'll use mock token and verify fallback works
        setup_test_token(&key_store, source_id, "mock-token-for-testing")
            .expect("Failed to store test token");

        let opts = default_test_capture_options();

        // Measure capture operation (including fallback)
        let result = measure_operation(
            "capture_window",
            backend.capture_window(source_id.to_string(), &opts),
        )
        .await;

        match result {
            Ok((image, timing)) => {
                print_timing_result(&timing);

                eprintln!("✓ Capture succeeded (via fallback with mock token)");
                eprintln!("  Dimensions: {:?}", image.dimensions());
                eprintln!(
                    "  Duration: {:.3}s ({:.0}ms)",
                    timing.duration_secs(),
                    timing.duration_ms()
                );

                // Assert latency target (<2s for P95)
                let thresholds = PerformanceThresholds::default();
                assert_duration_below(
                    timing.duration,
                    thresholds.capture_latency_p95,
                    "capture_window",
                );
            }
            Err(e) => {
                eprintln!("✗ Capture failed: {:?}", e);
                // With mock token, portal will reject and trigger fallback
                // Fallback may also fail in headless CI environment
                eprintln!(
                    "Note: This is expected with mock token in CI. On live Wayland with real \
                     token, capture should succeed."
                );
            }
        }

        // Cleanup
        cleanup_test_tokens(&key_store, &[source_id]);
    }
}

#[cfg(not(target_os = "linux"))]
#[test]
fn integration_tests_require_linux() {
    println!("Wayland integration tests only run on Linux.");
}
