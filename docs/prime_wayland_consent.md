# Prime Wayland Consent Guide

Complete guide to using `prime_wayland_consent` for headless screenshot capture on Wayland.

## What is Priming?

Wayland's security model prevents applications from capturing screens without explicit user permission. The **prime consent workflow** solves this by:

1. **One-time permission request**: User grants permission via portal dialog
2. **Token storage**: Restore token stored securely (keyring or encrypted file)
3. **Headless captures**: Subsequent captures use stored token (no user prompt)

This enables AI coding agents to capture screenshots programmatically after initial authorization.

## Why is it Needed?

Unlike X11, Wayland enforces strict security:
- No window enumeration without permission
- No screen capture without explicit consent
- Tokens expire on compositor restart

Priming establishes trust once, enabling seamless headless operation afterward.

---

## Prerequisites

### 1. Install XDG Desktop Portal

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install xdg-desktop-portal xdg-desktop-portal-gtk pipewire
```

**Fedora:**
```bash
sudo dnf install xdg-desktop-portal xdg-desktop-portal-gtk pipewire
```

**Arch Linux:**
```bash
sudo pacman -S xdg-desktop-portal xdg-desktop-portal-gtk pipewire
```

### 2. Verify Portal is Running

```bash
systemctl --user status xdg-desktop-portal
```

If not running:
```bash
systemctl --user enable --now xdg-desktop-portal
systemctl --user restart xdg-desktop-portal
```

### 3. Check PipeWire

```bash
systemctl --user status pipewire
```

If not running:
```bash
systemctl --user enable --now pipewire
```

---

## Using Prime Consent

### Step 1: Call prime_wayland_consent Tool

**MCP Request:**
```json
{
  "method": "tools/call",
  "params": {
    "name": "prime_wayland_consent",
    "arguments": {
      "source_id": "wayland-default",
      "source_type": "monitor",
      "include_cursor": false
    }
  }
}
```

**Parameters:**
- `source_id` (string): Identifier for this capture source (e.g., "wayland-default")
- `source_type` (string): "monitor", "window", or "virtual" (default: "monitor")
- `include_cursor` (boolean): Include mouse cursor in captures (default: false)

### Step 2: Grant Permission in Portal Dialog

A system dialog will appear asking for screen sharing permission:

**GNOME:** "Allow [application] to record the screen?"
**KDE Plasma:** "Share your screen with [application]?"

Click **"Allow"** or **"Share"** to grant permission.

### Step 3: Verify Success

**MCP Response:**
```json
{
  "status": "success",
  "source_id": "wayland-default",
  "num_streams": 1,
  "next_steps": "Use capture_window with this source_id for headless captures"
}
```

**Token Storage:**
- Platform keyring: `gnome-keyring` or `kwallet` (preferred)
- Encrypted file: `~/.local/share/screenshot-mcp/token-store.enc` (fallback)

---

## Capturing After Priming

Once primed, use `capture_window` with the `source_id`:

**MCP Request:**
```json
{
  "method": "tools/call",
  "params": {
    "name": "capture_window",
    "arguments": {
      "selector": "wayland:wayland-default"
    }
  }
}
```

**Expected:** Capture succeeds without user prompt (headless operation).

---

## Troubleshooting

### Portal Dialog Doesn't Appear

**Symptoms:** `prime_wayland_consent` hangs or times out after 30s.

**Solutions:**
1. Check portal is running: `systemctl --user status xdg-desktop-portal`
2. Restart portal: `systemctl --user restart xdg-desktop-portal`
3. Check compositor logs: `journalctl --user -u xdg-desktop-portal -f`
4. Ensure running in desktop session (not via SSH)

### Permission Denied Error

**Symptoms:** Error: "User denied screen capture permission"

**Solutions:**
1. Re-run `prime_wayland_consent` and click "Allow" this time
2. Check portal backend is installed:
   - GNOME: `xdg-desktop-portal-gtk`
   - KDE: `xdg-desktop-portal-kde`
   - wlroots: `xdg-desktop-portal-wlr`

### Capture Timeout After Priming

**Symptoms:** First prime succeeds, but subsequent captures timeout.

**Solutions:**
1. Check PipeWire is running: `systemctl --user status pipewire`
2. Restart PipeWire: `systemctl --user restart pipewire`
3. Try fallback capture (should prompt for display permission)

### Token Expired After Restart

**Symptoms:** Captures work, then fail after compositor restart.

**Cause:** Compositor restart invalidates all restore tokens.

**Solution:** Re-run `prime_wayland_consent` to obtain new token. This is expected behavior on Wayland.

### Keyring Unavailable Warning

**Symptoms:** Log message: "Keyring unavailable, using encrypted file storage"

**Impact:** Minimal - tokens stored in encrypted file instead of keyring.

**Fix (optional):**
```bash
# Install keyring (GNOME)
sudo apt install gnome-keyring

# Install keyring (KDE)
sudo apt install kwalletmanager
```

---

## Security Considerations

### Token Storage

**Keyring (Preferred):**
- Stored in system keyring (gnome-keyring, kwallet)
- Protected by user login credentials
- Encrypted at rest

**File Fallback:**
- Stored at `~/.local/share/screenshot-mcp/token-store.enc`
- ChaCha20-Poly1305 authenticated encryption
- File permissions: 0600 (owner read/write only)
- Key derived from hostname + username (HKDF-SHA256)

### Token Lifecycle

- **Creation:** User grants permission → token stored
- **Usage:** Token retrieved for each capture → rotated after use
- **Rotation:** Single-use tokens automatically rotated (security feature)
- **Expiration:** Invalidated on compositor restart

### Revoking Permission

**Via System Settings:**
- GNOME: Settings → Privacy → Screen Sharing
- KDE: System Settings → Personalization → Applications

**Via Command Line:**
```bash
# Delete stored token
rm -rf ~/.local/share/screenshot-mcp/
```

---

## Performance Expectations

Based on M2 validation:

- **Prime consent flow:** <5s (excluding time waiting for user)
- **Headless capture:** <2s (P95 latency)
- **Token rotation:** <100ms overhead

See [docs/TESTING.md](./TESTING.md) for performance validation details.

---

## Advanced Configuration

### Using Multiple Source IDs

Prime different sources for different capture scenarios:

```json
// Prime monitor capture
{
  "name": "prime_wayland_consent",
  "arguments": {
    "source_id": "monitor-capture",
    "source_type": "monitor"
  }
}

// Prime window capture
{
  "name": "prime_wayland_consent",
  "arguments": {
    "source_id": "window-capture",
    "source_type": "window"
  }
}
```

Then use appropriate `source_id` in `capture_window` calls.

### Including Cursor in Captures

```json
{
  "name": "prime_wayland_consent",
  "arguments": {
    "source_id": "with-cursor",
    "include_cursor": true
  }
}
```

---

## Compositor-Specific Notes

### GNOME Shell (40+)
- ✅ Most stable, recommended
- Portal UI: Modal dialog with preview
- Token persistence: Excellent

### KDE Plasma (5.27+)
- ✅ Fully supported
- Portal UI: Sidebar picker with preview
- Token persistence: Excellent

### wlroots (Sway, Hyprland, etc.)
- ⚠️ Limited support
- Requires: `xdg-desktop-portal-wlr`
- Token persistence: Variable by compositor

### Compatibility Matrix

| Compositor | Portal Backend | Prime Support | Token Rotation |
|------------|---------------|---------------|----------------|
| GNOME 40+ | xdg-desktop-portal-gtk | ✅ Full | ✅ Yes |
| KDE Plasma 5.27+ | xdg-desktop-portal-kde | ✅ Full | ✅ Yes |
| Sway 1.5+ | xdg-desktop-portal-wlr | ⚠️ Partial | ❌ Limited |
| Hyprland | xdg-desktop-portal-wlr | ⚠️ Partial | ❌ Limited |

---

## Integration Examples

### Claude Desktop Config

```json
{
  "mcpServers": {
    "screenshot": {
      "command": "/path/to/screenshot-mcp",
      "env": {
        "RUST_LOG": "screenshot_mcp=info"
      }
    }
  }
}
```

### First-Time Workflow

1. Start Claude Desktop with screenshot-mcp configured
2. Ask Claude: "Prime Wayland consent for screenshots"
3. Claude calls `prime_wayland_consent`
4. You grant permission in portal dialog
5. Claude confirms: "Permission granted, ready for captures"
6. Future screenshot requests work headlessly

### Automated Testing

```bash
# Prime consent (manual - requires user)
./scripts/run_wayland_integration_tests.sh test_prime_consent_success

# Run performance suite (includes priming)
./scripts/run_performance_suite.sh
```

---

## FAQ

**Q: Do I need to prime every time I restart my computer?**
A: No, but you need to re-prime if you restart your Wayland compositor (logout/login).

**Q: Can I use screenshot-mcp over SSH?**
A: No, portal dialogs require a live desktop session. Use X11 forwarding or run locally.

**Q: Does this work on X11?**
A: Not needed on X11. X11 backend (M3) will capture without priming.

**Q: What happens if I deny permission?**
A: Captures will fail with PermissionDenied error. Re-run `prime_wayland_consent` to try again.

**Q: How do I check if I've already primed?**
A: Call `list_windows` - primed sources appear in the list.

---

## See Also

- [README.md](../README.md) - Project overview
- [TESTING.md](./TESTING.md) - Integration testing guide
- [acceptance-checklist.md](./acceptance-checklist.md) - M2 validation tests
- [XDG Desktop Portal Spec](https://flatpak.github.io/xdg-desktop-portal/) - Portal API documentation
