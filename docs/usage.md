# Usage Guide

API reference and workflows for screenshot-mcp.

## Tools at a Glance

| Tool | Returns | When to Use |
|------|---------|-------------|
| `health_check` | platform, backend, ok | First call - detect environment |
| `list_windows` | id, title, class, owner | Find capture targets |
| `capture_window` | image + file path | Take screenshot |
| `prime_wayland_consent` | token stored | Wayland only - one-time setup |

---

## health_check

Detects platform and backend status.

**Request:**
```json
{ "name": "health_check", "arguments": {} }
```

**Response:**
```json
{ "platform": "linux", "backend": "wayland", "ok": true }
```

---

## list_windows

Enumerates visible windows with metadata.

**Parameters:**

| Name | Type | Description |
|------|------|-------------|
| `filter.title` | string | Substring match on title |
| `filter.class` | string | Window class/type |
| `filter.owner` | string | Process/application name |

**Request:**
```json
{ "name": "list_windows", "arguments": {} }
```

**Response:**
```json
[
  {
    "id": "12345",
    "title": "Mozilla Firefox",
    "class": "firefox",
    "owner": "firefox",
    "pid": 1000,
    "backend": "wayland"
  }
]
```

---

## capture_window

Captures a screenshot of a specific window.

**Parameters:**

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `selector.title` | string | - | Window title substring |
| `selector.class` | string | - | Window class |
| `selector.exe` | string | - | Executable name (Windows) |
| `selector.id` | string | - | Exact window ID |
| `format` | string | `"png"` | `"png"`, `"webp"`, or `"jpeg"` |
| `quality` | number | 80 | 0-100 (jpeg/webp only) |
| `scale` | number | 1.0 | 0.1-2.0 resize factor |
| `region.x/y/width/height` | number | - | Crop region in pixels |

**Request:**
```json
{
  "name": "capture_window",
  "arguments": {
    "selector": { "title": "Firefox" },
    "format": "jpeg",
    "quality": 80,
    "scale": 0.5
  }
}
```

**Response:**
```json
{
  "content": [
    { "type": "image", "mimeType": "image/jpeg", "data": "..." },
    { "type": "text", "text": "Saved to: /tmp/screenshot_123.jpg" }
  ]
}
```

---

## prime_wayland_consent

**Wayland only.** Opens the portal picker to obtain permission and store a restore token.

**Parameters:**

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `source` | string | `"window"` | `"window"` or `"display"` |

**Request:**
```json
{
  "name": "prime_wayland_consent",
  "arguments": { "source": "window" }
}
```

**User Action:** A system dialog appears. Select a window/screen and check "Allow restore" if available.

**Response:**
```json
{
  "status": "success",
  "source_id": "wayland-default",
  "next_steps": "Use capture_window for headless captures"
}
```

**Token Storage:**
- Keyring (gnome-keyring/kwallet) — preferred
- Encrypted file (`~/.local/share/screenshot-mcp/token-store.enc`) — fallback

---

## Workflows

### Basic Capture (Windows/X11)

```mermaid
sequenceDiagram
    participant Agent
    participant MCP
    participant OS
    
    Agent->>MCP: health_check
    MCP-->>Agent: {platform: "windows", ok: true}
    Agent->>MCP: list_windows
    MCP->>OS: EnumWindows / EWMH
    OS-->>MCP: window list
    MCP-->>Agent: [{id, title, class}...]
    Agent->>MCP: capture_window {title: "Notepad"}
    MCP->>OS: WGC / xcap
    OS-->>MCP: raw pixels
    MCP-->>Agent: {image, file_path}
```

### Wayland Capture

```mermaid
sequenceDiagram
    participant Agent
    participant MCP
    participant Portal
    participant User
    
    Agent->>MCP: health_check
    MCP-->>Agent: {backend: "wayland"}
    Agent->>MCP: prime_wayland_consent
    MCP->>Portal: org.freedesktop.portal.ScreenCast
    Portal->>User: "Allow screen sharing?"
    User-->>Portal: Grant + check "Allow restore"
    Portal-->>MCP: restore token
    MCP-->>Agent: {status: "success"}
    
    Note over Agent,MCP: Subsequent captures are headless
    Agent->>MCP: capture_window {title: "Firefox"}
    MCP->>Portal: Use restore token
    Portal-->>MCP: frame
    MCP-->>Agent: {image, file_path}
```

---

## Error Codes

| Code | Name | Description | Remediation |
|------|------|-------------|-------------|
| -32000 | PortalUnavailable | XDG Portal not found | Install `xdg-desktop-portal` |
| -32001 | WindowNotFound | Window doesn't exist | Run `list_windows` first |
| -32002 | PermissionDenied | Capture denied | Re-run `prime_wayland_consent` |
| -32003 | CaptureTimeout | Capture took too long | Check GPU/drivers |
| -32004 | EncodingError | Image encoding failed | Try different format |
| -32005 | TokenStoreError | Token save/load failed | Check keyring access |
| -32006 | PlatformError | General platform error | See logs |

**Error Response Example:**
```json
{
  "code": -32001,
  "message": "Window 'Notepad' not found",
  "data": { "hint": "Run list_windows to see available targets." }
}
```

---

## Best Practices

1. **Always call `health_check` first** — know your platform/backend
2. **List before capture** — don't guess window titles
3. **Use cropping** — reduce payload size, exclude sensitive areas
4. **Prefer JPEG** — faster encoding, smaller size for most UI
5. **Handle token expiry** — Wayland tokens invalidate on compositor restart

---

## Platform Notes

### Linux - Wayland

- Requires one-time `prime_wayland_consent`
- Token expires on logout/compositor restart
- Install portal: `sudo apt install xdg-desktop-portal xdg-desktop-portal-gtk`

### Linux - X11

- Works directly via EWMH, no consent needed
- Ensure `DISPLAY` is set (usually `:0`)

### Windows

- Uses Windows Graphics Capture API (Windows 10 1803+)
- Yellow security border appears around captured window (OS feature)
- Cannot capture minimized windows
