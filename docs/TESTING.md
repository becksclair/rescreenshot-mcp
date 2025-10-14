# Testing Guide

Comprehensive testing documentation for screenshot-mcp M2 (Wayland Backend).

## Test Organization

The project has three types of tests:

1. **Unit Tests** (229 tests) - Fast, no external dependencies
2. **Integration Tests** (6 tests) - Require live Wayland session
3. **Performance Tests** - Measure M2 exit criteria metrics

## Quick Start

```bash
# Run all unit tests (CI-safe, no Wayland required)
cargo test --all-features

# Compile integration tests without running
cargo test --features integration-tests --no-run

# Run integration tests (requires Wayland session)
./scripts/run_wayland_integration_tests.sh

# Run specific integration test
./scripts/run_wayland_integration_tests.sh test_prime_consent
```

## Unit Tests

### Running Unit Tests

```bash
# All tests with all features
cargo test --all-features

# Specific test module
cargo test --test-threads=1 util::key_store

# With output
cargo test -- --nocapture
```

### Test Coverage

- **KeyStore** (12 tests): Token storage, rotation, persistence
- **Wayland Types** (19 tests): SourceType, PersistMode, WaylandSource
- **WaylandBackend** (30+ tests): Capture, resolve_target, list_windows
- **Error Handling** (10 tests): Timeout, fallback, portal errors
- **MCP Tools** (15+ tests): health_check, list_windows, capture_window

**Total**: 229 unit tests, all passing

## Integration Tests

Integration tests validate end-to-end workflows with real Wayland portals. They are marked with `#[ignore]` and require manual execution.

### Prerequisites

Before running integration tests:

1. **Wayland Compositor**
   - GNOME Shell 40+, KDE Plasma 5.27+, or Sway 1.5+
   - Set `WAYLAND_DISPLAY` environment variable

2. **XDG Desktop Portal**
   ```bash
   # Install portal and backend
   sudo apt install xdg-desktop-portal xdg-desktop-portal-gtk  # GNOME
   sudo apt install xdg-desktop-portal xdg-desktop-portal-kde  # KDE
   sudo apt install xdg-desktop-portal xdg-desktop-portal-wlr  # wlroots
   ```

3. **PipeWire**
   ```bash
   sudo apt install pipewire
   systemctl --user enable --now pipewire
   ```

4. **DBus Session**
   - Ensure `DBUS_SESSION_BUS_ADDRESS` is set
   - Run tests in your desktop session, not via SSH

### Running Integration Tests

Use the provided script:

```bash
# All integration tests
./scripts/run_wayland_integration_tests.sh

# Specific test
./scripts/run_wayland_integration_tests.sh test_prime_consent_success

# With Rust test output
cargo test --features integration-tests -- --ignored --nocapture
```

### Integration Test Catalog

| Test | Description | User Interaction Required |
|------|-------------|---------------------------|
| `test_prime_consent_success` | Prime consent with timing measurement | Yes - Grant permission |
| `test_capture_window_after_prime` | Capture with latency measurement | Yes - Grant fallback permission |
| `test_full_workflow_token_expired` | Token expiration and rotation | Manual - Revoke permission |
| `test_full_workflow_compositor_restart` | Compositor restart handling | Manual - Logout/login |
| `test_full_workflow_permission_denied` | Permission denial handling | Yes - Deny permission |
| `test_full_workflow_portal_timeout` | Portal timeout handling | Yes - Don't respond for 30s |

### Expected Results

- **Prime Consent**: Stores token, returns source_id, completes in <5s (excluding user interaction)
- **Capture After Prime**: Headless capture in <2s (with valid token)
- **Token Expired**: Fallback to display capture + region crop
- **Compositor Restart**: Re-prime required, fallback works
- **Permission Denied**: Clear error message with retry instructions
- **Portal Timeout**: CaptureTimeout after 30s

## Performance Testing

Performance tests validate M2 exit criteria:

### Performance Targets

| Metric | Target | Test Method |
|--------|--------|-------------|
| Prime consent flow | <5s (excluding user) | Integration test timing |
| Headless capture (P95) | <2s | measure-capture tool |
| Token rotation | <100ms | Integration test timing |
| Memory peak | <200MB | Memory probe script |
| Memory leaks | None after 10 captures | Memory probe script |

### Running Performance Tests

```bash
# Performance measurement tool (stub - Task 11)
cargo run --bin measure-capture --features perf-tests,linux-wayland

# Memory profiling (stub - Task 13)
./scripts/run_memory_probe.sh

# Performance suite (stub - Task 12)
./scripts/run_performance_suite.sh
```

## Manual Acceptance Tests

For M2 exit criteria validation, execute these manual tests:

### T-M2-01: Fresh Install → Prime Consent → Token Stored

**Steps:**
1. Clean install: `rm -rf ~/.local/share/screenshot-mcp/`
2. Run: `./scripts/run_wayland_integration_tests.sh test_prime_consent_success`
3. Grant permission in portal dialog
4. Verify: Token stored in `~/.local/share/screenshot-mcp/token-store.enc` or system keyring

**Expected:** Token stored successfully, consent completes in <5s

### T-M2-02: Restart Process → Capture Headlessly in <2s

**Steps:**
1. Prime consent (T-M2-01)
2. Restart terminal/process
3. Run: `./scripts/run_wayland_integration_tests.sh test_capture_window_after_prime`
4. Verify: No user prompt, capture completes quickly

**Expected:** Headless capture in <2s (P95)

### T-M2-03: Simulate Compositor Restart → Re-prompt, Store New Token

**Steps:**
1. Prime consent (T-M2-01)
2. Logout and login (or restart compositor)
3. Run: `./scripts/run_wayland_integration_tests.sh test_full_workflow_compositor_restart`
4. Grant permission when prompted

**Expected:** Old token invalid, new token stored, fallback works

### T-M2-04: Restore Fails → Display Capture + Region Crop Succeeds

**Steps:**
1. Store invalid token: `echo "invalid" > ~/.local/share/screenshot-mcp/token-store.enc`
2. Run: `./scripts/run_wayland_integration_tests.sh test_capture_window_after_prime`
3. Grant fallback permission

**Expected:** Falls back to display capture, applies region crop, succeeds

### T-M2-05: Keyring Unavailable → Fallback to File, Warning Logged

**Steps:**
1. Disable keyring: `export NO_KEYRING=1` (implementation-dependent)
2. Prime consent
3. Check logs for fallback warning

**Expected:** File-based storage used, warning logged, functionality intact

## Continuous Integration

CI runs automatically on push/PR:

```yaml
# .github/workflows/ci.yml
jobs:
  test:
    - Run unit tests: cargo test --all-features
    - Compile integration tests: cargo test --features integration-tests --no-run
    - Run clippy: cargo clippy --all-targets --all-features -- -D warnings
    - Check formatting: cargo fmt --check
  build:
    - Build release: cargo build --release --all-features
    - Build measure-capture: cargo build --bin measure-capture --features perf-tests,linux-wayland
```

**Note:** CI does NOT execute integration tests (no Wayland environment).

## Troubleshooting

### Integration Tests Fail with "Portal Unavailable"

**Cause:** xdg-desktop-portal not running

**Solution:**
```bash
# Check portal service
systemctl --user status xdg-desktop-portal

# Restart portal
systemctl --user restart xdg-desktop-portal

# Check logs
journalctl --user -u xdg-desktop-portal -f
```

### Integration Tests Fail with "Permission Denied"

**Cause:** User denied permission or portal backend missing

**Solution:**
- Grant permission when portal picker appears
- Install compositor-specific portal backend (gtk/kde/wlr)

### Integration Tests Fail with "Timeout"

**Cause:** Portal dialog not responded to within 30s

**Solution:**
- Respond quickly to portal dialogs
- Check compositor logs for portal issues

### Token Rotation Fails

**Cause:** Portal not returning new token

**Solution:**
- Verify compositor supports token rotation (KDE Plasma 5.27+)
- Check portal version: `xdg-desktop-portal --version`
- Update to latest portal version

### Memory Probe Fails

**Cause:** Valgrind not installed

**Solution:**
```bash
sudo apt install valgrind
```

## Test Development

### Adding New Unit Tests

```rust
#[test]
fn test_my_feature() {
    // Arrange
    let backend = create_test_backend();

    // Act
    let result = backend.my_feature();

    // Assert
    assert!(result.is_ok());
}
```

### Adding New Integration Tests

```rust
#[tokio::test]
#[ignore = "Requires live Wayland session"]
async fn test_my_integration() {
    use common::wayland_harness::*;

    print_test_environment();
    let backend = create_test_backend();

    // Use timing measurement
    let result = measure_operation(
        "my_operation",
        backend.my_operation()
    ).await;

    match result {
        Ok((value, timing)) => {
            print_timing_result(&timing);
            // assertions
        }
        Err(e) => panic!("Operation failed: {}", e),
    }
}
```

## Test Maintenance

### Cleaning Test Artifacts

```bash
# Remove test tokens
rm -rf ~/.local/share/screenshot-mcp/

# Clean build artifacts
cargo clean

# Remove temp files
find /tmp -name "screenshot-*.png" -delete
```

### Updating Test Baselines

When changing functionality:

1. Run unit tests: `cargo test --all-features`
2. Fix failing tests or update expectations
3. Compile integration tests: `cargo test --features integration-tests --no-run`
4. Run integration tests manually: `./scripts/run_wayland_integration_tests.sh`
5. Update documentation if test behavior changes

## Test Matrix

| Environment | Unit Tests | Integration Tests | Performance Tests |
|-------------|------------|-------------------|-------------------|
| CI (GitHub Actions) | ✅ Run | ✅ Compile only | ✅ Compile only |
| Developer (Wayland) | ✅ Run | ✅ Run manually | ✅ Run manually |
| Developer (X11/macOS) | ✅ Run | ❌ Skip | ❌ Skip |

## References

- [M2 TODO](../TODO.md) - Exit criteria and implementation notes
- [Integration Test Runner](../scripts/run_wayland_integration_tests.sh) - Automated test execution
- [Test Harness](../tests/common/wayland_harness.rs) - Shared test utilities
- [XDG Desktop Portal Spec](https://flatpak.github.io/xdg-desktop-portal/) - Portal API reference
