# Troubleshooting

Common issues and solutions for screenshot-mcp.

---

## General

### Server fails to start

```bash
# Run manually to see errors
./screenshot-mcp

# Enable debug logs
RUST_LOG=debug ./screenshot-mcp
```

### JSON parse errors in MCP client

**Error:** `Unexpected token 'I' in JSON at position 0`

**Cause:** Logs printing to stdout corrupt the JSON-RPC stream.

**Fix:**
1. Update to latest version (logs go to stderr)
2. Don't redirect stderr to stdout in your config
3. Set `RUST_LOG=off` as workaround

---

## Linux - Wayland

### Portal unavailable

**Error:** `org.freedesktop.DBus.Error.ServiceUnknown`

```bash
# Install portal
sudo apt install xdg-desktop-portal xdg-desktop-portal-gtk

# Restart portal
systemctl --user restart xdg-desktop-portal
```

### Permission denied during capture

**Cause:** Token invalid/missing, or user denied dialog.

**Fix:**
1. Call `prime_wayland_consent` again
2. Click "Share" in the dialog
3. Check keyring is unlocked

### Portal dialog doesn't appear

**Cause:** Running via SSH or in headless session.

**Fix:** Run in a live desktop session. Portal dialogs require user interaction.

### Token expired after restart

**Cause:** Compositor restart invalidates all tokens.

**Fix:** Re-run `prime_wayland_consent`. This is expected behavior.

### Screenshots are black/blank

**Cause:** Compositor restrictions or PipeWire issue.

**Fix:**
1. Test screen sharing in another app (OBS, browser)
2. Update compositor and portal packages
3. Restart PipeWire: `systemctl --user restart pipewire`

---

## Linux - X11

### Display not found

**Error:** `XOpenDisplay failed`

```bash
# Check DISPLAY
echo $DISPLAY  # Should be :0 or similar

# Set if missing
export DISPLAY=:0
```

### Window list is empty

**Cause:** Window manager doesn't support EWMH, or all windows minimized.

**Fix:** Use a standard WM (GNOME, KDE, i3). Ensure windows are visible.

### Screenshots are black/blank

**Cause:** Window is offscreen or using non-standard rendering.

**Fix:** Move window fully onto visible display area.

---

## Windows

### Window not found - but it's open

**Causes:**
1. Window is minimized (WGC can't capture minimized windows)
2. App running as Admin, MCP server is not

**Fix:**
- Restore the window to desktop
- Run Claude Desktop as Administrator

### Yellow border around captured window

This is a Windows security feature for Graphics Capture API. It cannot be disabled.

### "Linker not found" during build

**Fix:** Install Visual Studio C++ Build Tools with "Desktop development with C++" workload.

### Access denied when running

**Fix:** Add exclusion for `screenshot-mcp.exe` in Windows Defender.

### Capture timeout on 4K displays

**Cause:** Weak GPU or driver hang.

**Fix:**
- Use `scale: 0.5` to reduce frame size
- Update GPU drivers
- Close overlay apps (Discord, OBS, GeForce Experience)

---

## Build Errors

### "linker 'cc' not found" (Linux)

```bash
sudo apt install build-essential  # Debian/Ubuntu
sudo dnf install gcc             # Fedora
```

### pkg-config errors

**Fix:** Install development headers. See [setup.md](./setup.md) for full package list.

---

## Quick Diagnostic Commands

```bash
# Linux - Check portal
systemctl --user status xdg-desktop-portal

# Linux - Check PipeWire  
systemctl --user status pipewire

# Linux - Check X11
xset q

# Windows - Check DirectX
dxdiag

# Windows - Check build number (needs 17134+)
winver
```
