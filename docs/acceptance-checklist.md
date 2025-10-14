# M2 Acceptance Test Checklist

Manual acceptance tests for Wayland Backend with Restore Tokens milestone.

## Test Execution Status

| Test ID | Description | Status | Result | Date | Notes |
|---------|-------------|--------|--------|------|-------|
| T-M2-01 | Fresh install → prime consent → token stored | ⏳ Pending | - | - | - |
| T-M2-02 | Restart process → capture headlessly | ⏳ Pending | - | - | - |
| T-M2-03 | Compositor restart → re-prompt | ⏳ Pending | - | - | - |
| T-M2-04 | Restore fails → fallback succeeds | ⏳ Pending | - | - | - |
| T-M2-05 | Keyring unavailable → file fallback | ⏳ Pending | - | - | - |

Legend:
- ⏳ Pending - Not yet executed
- 🏃 In Progress - Currently testing
- ✅ Pass - Test passed all requirements
- ❌ Fail - Test failed, requires investigation
- ⚠️ Partial - Test passed with caveats

---

## T-M2-01: Fresh Install → Prime Consent → Token Stored

**Objective:** Verify fresh installation can obtain and store restore token

**Prerequisites:**
- Clean system state (no existing tokens)
- Wayland compositor running (KDE Plasma 5.27+ or GNOME 40+)
- xdg-desktop-portal and backend installed

**Steps:**
1. Clean existing state: `rm -rf ~/.local/share/screenshot-mcp/`
2. Run integration test: `./scripts/run_wayland_integration_tests.sh test_prime_consent_success`
3. Grant permission when portal dialog appears
4. Verify token stored: Check `~/.local/share/screenshot-mcp/token-store.enc` exists OR keyring entry

**Expected Result:**
- Portal dialog displays for user consent
- Token successfully stored after permission granted
- Operation completes in <5s (excluding user interaction time)

**Actual Result:**
- Status: ⏳ Pending
- Duration: -
- Notes: -

---

## T-M2-02: Restart Process → Capture Headlessly in <2s

**Objective:** Verify headless capture works after process restart

**Prerequisites:**
- Valid restore token from T-M2-01
- Same compositor session

**Steps:**
1. Ensure T-M2-01 passed (token exists)
2. Restart terminal/shell session
3. Run integration test: `./scripts/run_wayland_integration_tests.sh test_capture_window_after_prime`
4. Verify no user prompt appears
5. Measure capture latency

**Expected Result:**
- No portal dialog (headless operation)
- Capture completes successfully
- Latency <2s (P95)

**Actual Result:**
- Status: ⏳ Pending
- Latency: -
- Notes: -

---

## T-M2-03: Simulate Compositor Restart → Re-prompt, Store New Token

**Objective:** Verify graceful handling of compositor restart (token invalidation)

**Prerequisites:**
- Valid restore token from T-M2-01
- Ability to restart compositor (logout/login or compositor reload)

**Steps:**
1. Prime consent and capture successfully (T-M2-01, T-M2-02)
2. Restart Wayland compositor (e.g., logout and login)
3. Run integration test: `./scripts/run_wayland_integration_tests.sh test_full_workflow_compositor_restart`
4. Verify fallback to display capture OR re-prime prompt
5. Grant permission if prompted

**Expected Result:**
- Old token invalid after compositor restart
- Falls back to display capture + region crop OR prompts for re-prime
- New token stored if re-primed

**Actual Result:**
- Status: ⏳ Pending
- Fallback behavior: -
- Notes: -

---

## T-M2-04: Restore Fails → Display Capture + Region Crop Succeeds

**Objective:** Verify fallback strategy when restore token fails

**Prerequisites:**
- Live Wayland session

**Steps:**
1. Manually invalidate token: `echo "invalid-token-data" > ~/.local/share/screenshot-mcp/token-store.enc`
2. Run integration test with capture request
3. Grant fallback permission when prompted
4. Verify display capture + region cropping applied

**Expected Result:**
- Token restore fails gracefully
- Falls back to display capture
- Region crop applied correctly
- Capture succeeds

**Actual Result:**
- Status: ⏳ Pending
- Fallback triggered: -
- Notes: -

---

## T-M2-05: Keyring Unavailable → Fallback to File, Warning Logged

**Objective:** Verify encrypted file fallback when platform keyring unavailable

**Prerequisites:**
- Ability to disable keyring (e.g., headless environment, or uninstall gnome-keyring/kwallet)

**Steps:**
1. Disable keyring access (environment-dependent)
2. Prime consent
3. Check logs for fallback warning
4. Verify token stored in `~/.local/share/screenshot-mcp/token-store.enc`
5. Verify file permissions are 0600

**Expected Result:**
- Keyring unavailable detected
- Warning logged: "Keyring unavailable, using encrypted file storage"
- Token stored in encrypted file (ChaCha20-Poly1305)
- File permissions restrict access (0600)

**Actual Result:**
- Status: ⏳ Pending
- Fallback detected: -
- Notes: -

---

## Performance Validation

| Metric | Target | Measured | Result |
|--------|--------|----------|--------|
| Prime consent flow | <5s | - | ⏳ Pending |
| Headless capture (P95) | <2s | - | ⏳ Pending |
| Token rotation | <100ms | - | ⏳ Pending |
| Memory peak | <200MB | - | ⏳ Pending |
| Memory leaks | None | - | ⏳ Pending |

Run performance suite: `./scripts/run_performance_suite.sh`
Run memory probe: `./scripts/run_memory_probe.sh 10`

---

## Sign-Off

**Tested By:** _________________
**Date:** _______________
**Environment:** _______________
**Compositor:** _______________
**Overall Result:** ⏳ Pending

**Notes:**



**M2 Exit Criteria Met:** ☐ Yes ☐ No ☐ Partial

---

## Instructions for Testers

1. Execute tests sequentially (T-M2-01 through T-M2-05)
2. Record results in this document
3. Save performance test outputs to `perf-results/` directory
4. Document any unexpected behavior in Notes section
5. Sign off when all tests complete

For questions or issues, see [TESTING.md](./TESTING.md) troubleshooting section.
