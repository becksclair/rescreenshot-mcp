# screenshot-mcp API Documentation

**Version:** 0.1.0 (M0)
**Protocol:** Model Context Protocol (MCP)
**Transport:** stdio
**Last Updated:** 2025-10-13

---

## Overview

screenshot-mcp provides a Model Context Protocol (MCP) server for capturing screenshots across different platforms. This document describes the available tools, their parameters, and response formats.

---

## Connection

### Transport: stdio

screenshot-mcp communicates via stdin/stdout using the MCP protocol over JSON-RPC 2.0.

**Starting the Server:**
```bash
/path/to/screenshot-mcp
```

**Environment Variables:**
- `RUST_LOG`: Logging level (default: `screenshot_mcp=info`)
  - Values: `error`, `warn`, `info`, `debug`, `trace`
  - Example: `RUST_LOG=screenshot_mcp=debug`

---

## Available Tools (M0)

### health_check

**Status:** ✅ Available (M0)
**Description:** Checks server health and detects the current platform and display backend.

#### Request

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "health_check",
    "arguments": {}
  }
}
```

**Parameters:** None

#### Response

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"platform\":\"linux\",\"backend\":\"wayland\",\"ok\":true}"
      }
    ],
    "isError": false
  }
}
```

**Response Schema:**

```typescript
{
  platform: string;  // "linux" | "windows" | "macos" | "unknown"
  backend: string;   // "wayland" | "x11" | "windows" | "macos" | "none"
  ok: boolean;       // true if server is healthy
}
```

**Backend Values:**
- `wayland` - Linux with Wayland compositor (detected via $WAYLAND_DISPLAY)
- `x11` - Linux with X11 display server (detected via $DISPLAY)
- `windows` - Windows 10/11 platform
- `macos` - macOS 12+ platform
- `none` - No display backend detected

#### Examples

**Linux with Wayland:**
```json
{
  "platform": "linux",
  "backend": "wayland",
  "ok": true
}
```

**Linux with X11:**
```json
{
  "platform": "linux",
  "backend": "x11",
  "ok": true
}
```

**Windows:**
```json
{
  "platform": "windows",
  "backend": "windows",
  "ok": true
}
```

**macOS:**
```json
{
  "platform": "macos",
  "backend": "macos",
  "ok": true
}
```

**Headless/SSH (No Display):**
```json
{
  "platform": "linux",
  "backend": "none",
  "ok": true
}
```

#### Error Handling

If serialization fails (unlikely):
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32603,
    "message": "Failed to serialize health check response: ...",
    "data": null
  }
}
```

#### Use Cases

1. **Verify Server Connectivity:**
   - Confirm the MCP server is running and responsive
   - Check that stdio communication is working

2. **Platform Detection:**
   - Determine which platform-specific features are available
   - Decide which capture method to use

3. **Diagnostic Information:**
   - Debug client-server communication issues
   - Verify environment detection

#### Performance

- **Response Time:** <10ms (typical)
- **Memory Usage:** Negligible
- **Side Effects:** None (read-only operation)

---

## Upcoming Tools (M1-M6)

The following tools are planned for future milestones:

### list_windows (M2-M5)

**Status:** 🚧 Planned
**Description:** Enumerates visible windows with metadata.

**Planned Parameters:**
```typescript
{
  filter?: {
    title?: string;      // Substring or regex
    class?: string;      // Window class/type
    owner?: string;      // Process/application name
  }
}
```

**Planned Response:**
```typescript
{
  windows: Array<{
    id: string;          // Platform-specific window ID
    title: string;       // Window title
    class: string;       // Window class
    owner: string;       // Process name
    pid: number;         // Process ID
    backend: string;     // Backend used for capture
  }>
}
```

### capture_window (M2-M5)

**Status:** 🚧 Planned
**Description:** Captures a screenshot of a specific window.

**Planned Parameters:**
```typescript
{
  selector: {
    title?: string;           // Window title (substring or regex)
    class?: string;           // Window class
    exe?: string;             // Executable name
  };
  format?: "png" | "webp" | "jpeg";  // Default: "png"
  quality?: number;                   // 0-100, applies to webp/jpeg
  scale?: number;                     // 0.1-2.0, resize factor
  includeCursor?: boolean;            // Include cursor in screenshot
  region?: {                          // Crop region
    x: number;
    y: number;
    width: number;
    height: number;
  };
}
```

**Planned Response:**
```typescript
{
  content: [
    {
      type: "image";
      mimeType: "image/png";
      data: string;  // Base64-encoded image
    },
    {
      type: "resource";
      resource: {
        uri: string;       // file:// path to temp file
        mimeType: string;  // image/png, image/webp, etc.
        size: number;      // File size in bytes
        title: string;     // Descriptive title
      }
    }
  ];
  metadata: {
    windowId: string;
    title: string;
    captureTime: string;  // ISO 8601 timestamp
    dimensions: {
      width: number;
      height: number;
    };
  };
}
```

### prime_wayland_consent (M2)

**Status:** 🚧 Planned (Wayland only)
**Description:** Opens the portal picker to obtain initial consent and store restore token.

**Planned Parameters:**
```typescript
{
  source?: "window" | "display";  // Default: "window"
}
```

**Planned Response:**
```typescript
{
  success: boolean;
  message: string;
  tokenStored: boolean;
}
```

---

## Data Types

### BackendType

```rust
pub enum BackendType {
    None,     // No backend detected
    Wayland,  // Wayland compositor
    X11,      // X11 display server
    Windows,  // Windows platform
    MacOS,    // macOS platform
}
```

**JSON Representation:**
```json
"none" | "wayland" | "x11" | "windows" | "macos"
```

### PlatformInfo

```rust
pub struct PlatformInfo {
    pub os: String,           // OS name
    pub backend: BackendType, // Display backend
}
```

**JSON Schema:**
```json
{
  "type": "object",
  "properties": {
    "os": {
      "type": "string"
    },
    "backend": {
      "type": "string",
      "enum": ["none", "wayland", "x11", "windows", "macos"]
    }
  },
  "required": ["os", "backend"]
}
```

### HealthCheckResponse

```rust
pub struct HealthCheckResponse {
    pub platform: String,  // OS name
    pub backend: String,   // Backend as string
    pub ok: bool,          // Health status
}
```

**JSON Schema:**
```json
{
  "type": "object",
  "properties": {
    "platform": {
      "type": "string"
    },
    "backend": {
      "type": "string"
    },
    "ok": {
      "type": "boolean"
    }
  },
  "required": ["platform", "backend", "ok"]
}
```

---

## Error Codes

### MCP Standard Errors

| Code | Name | Description |
|------|------|-------------|
| -32700 | Parse Error | Invalid JSON |
| -32600 | Invalid Request | Malformed request |
| -32601 | Method Not Found | Tool doesn't exist |
| -32602 | Invalid Params | Invalid parameters |
| -32603 | Internal Error | Server error |

### Custom Errors (Planned for M1+)

| Code | Name | Description |
|------|------|-------------|
| -32000 | PortalUnavailable | XDG Portal not found (Wayland) |
| -32001 | WindowNotFound | Window doesn't exist |
| -32002 | PermissionDenied | Screen recording permission denied |
| -32003 | CaptureTimeout | Capture took too long |
| -32004 | EncodingError | Image encoding failed |

---

## Rate Limits

**M0:** No rate limits implemented.

**Planned (M1+):**
- Max concurrent captures: 5
- Capture timeout: 30 seconds
- Temp file cleanup: On process exit

---

## Best Practices

### For MCP Clients

1. **Check Health First:**
   ```javascript
   const health = await client.callTool("health_check", {});
   console.log(`Platform: ${health.platform}, Backend: ${health.backend}`);
   ```

2. **Handle Platform Differences:**
   ```javascript
   if (health.backend === "wayland") {
     // May need to call prime_wayland_consent first (M2+)
   } else if (health.backend === "x11") {
     // Direct capture available (M3+)
   }
   ```

3. **Error Handling:**
   ```javascript
   try {
     const result = await client.callTool("health_check", {});
   } catch (error) {
     console.error("MCP error:", error.code, error.message);
   }
   ```

### For Server Administrators

1. **Logging:**
   ```bash
   # Enable debug logging
   RUST_LOG=screenshot_mcp=debug ./screenshot-mcp

   # Structured JSON logs
   RUST_LOG=screenshot_mcp=info ./screenshot-mcp 2>&1 | jq
   ```

2. **Permissions (Linux):**
   - Wayland: Ensure portal is installed (`xdg-desktop-portal-kde` or similar)
   - X11: User must have X server access ($DISPLAY set)

3. **Permissions (macOS - M5):**
   - Grant Screen Recording permission in System Settings
   - May require application restart

---

## Version History

### 0.1.0 (M0) - 2025-10-13
- Initial release
- `health_check` tool added
- Platform detection for Linux/Windows/macOS

### Upcoming

- **0.2.0 (M1):** Core capture infrastructure
- **0.3.0 (M2):** `prime_wayland_consent`, Wayland capture
- **0.4.0 (M3):** `list_windows`, X11 capture
- **0.5.0 (M4):** Windows capture
- **0.6.0 (M5):** macOS capture
- **1.0.0 (M6):** Full feature set

---

## Support

**Documentation:** See `README.md`, `TODO.md`, `STATUS.md`
**Issues:** GitHub Issues (post-v1.0 launch)
**License:** MIT (pending confirmation)

---

## Appendix: JSON-RPC Examples

### Full MCP Session Example

```json
# Client → Server: Initialize
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": {
      "name": "claude-desktop",
      "version": "1.0.0"
    }
  }
}

# Server → Client: Initialize Response
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": {}
    },
    "serverInfo": {
      "name": "screenshot-mcp",
      "version": "0.1.0"
    }
  }
}

# Client → Server: List Tools
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/list",
  "params": {}
}

# Server → Client: Tools List
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "tools": [
      {
        "name": "health_check",
        "description": "Check server health and detect platform/backend",
        "inputSchema": {
          "type": "object",
          "properties": {},
          "required": []
        }
      }
    ]
  }
}

# Client → Server: Call health_check
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "health_check",
    "arguments": {}
  }
}

# Server → Client: health_check Result
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"platform\":\"linux\",\"backend\":\"wayland\",\"ok\":true}"
      }
    ],
    "isError": false
  }
}
```

---

**Document Version:** 1.0
**Last Updated:** 2025-10-13
**Maintainer:** screenshot-mcp development team
