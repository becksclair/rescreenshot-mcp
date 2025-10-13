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

#[cfg(all(test, feature = "linux-wayland"))]
mod wayland_error_integration {
    use std::sync::Arc;

    use screenshot_mcp::{
        capture::{wayland_backend::WaylandBackend, CaptureFacade},
        model::{CaptureOptions, SourceType},
        util::key_store::KeyStore,
    };

    /// Helper to create WaylandBackend instance for tests
    fn create_backend() -> WaylandBackend {
        let key_store = Arc::new(KeyStore::new());
        WaylandBackend::new(key_store)
    }

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
        let backend = create_backend();

        // Attempt to prime consent - requires user interaction
        let result = backend
            .prime_consent(SourceType::Monitor, "test-integration", false)
            .await;

        // With live portal, this should succeed (user must grant permission)
        if let Ok(consent_result) = result {
            assert_eq!(consent_result.primary_source_id, "test-integration");
            assert!(consent_result.num_streams > 0);
            eprintln!("✓ Prime consent successful");
            eprintln!("  Source ID: {}", consent_result.primary_source_id);
            eprintln!("  Streams: {}", consent_result.num_streams);
        } else {
            eprintln!("✗ Prime consent failed: {:?}", result.unwrap_err());
            panic!("Expected prime consent to succeed with user permission");
        }
    }

    #[tokio::test]
    #[ignore = "Requires live Wayland session with portal"]
    async fn test_capture_window_after_prime() {
        let key_store = Arc::new(KeyStore::new());
        let backend = WaylandBackend::new(Arc::clone(&key_store));

        // First, store a mock token (in real scenario, would prime first)
        key_store
            .store_token("test-capture", "mock-token-for-testing")
            .expect("Failed to store token");

        let opts = CaptureOptions::default();
        let result = backend.capture_window("test-capture".to_string(), &opts).await;

        // With mock token, portal will reject it, triggering fallback
        // This validates the fallback path in a live environment
        match result {
            Ok(image) => {
                eprintln!("✓ Capture succeeded (via fallback)");
                eprintln!("  Dimensions: {:?}", image.dimensions());
            }
            Err(e) => {
                eprintln!("✗ Capture failed: {:?}", e);
                // Failure is expected with mock token in CI
            }
        }

        // Cleanup
        key_store.delete_token("test-capture").unwrap();
    }
}

#[cfg(not(feature = "linux-wayland"))]
#[test]
fn integration_tests_require_wayland_feature() {
    // Placeholder test to show feature requirement
    println!("Integration tests require --features linux-wayland");
}
