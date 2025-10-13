
# Build a Cross-Platform Screenshot MCP Server in Rust

**Role:** You are an elite Rust engineer. Deliver a production-grade, cross-platform MCP **stdio** server named `screenshot-mcp` that can **capture a specific application’s window** and **return the image** to the calling coding agent. Optimize for correctness, reliability, and minimal UX friction on **Wayland/X11**, **Windows**, and **macOS**. Ship working code, tests, and docs in one pass.

### Mission & Requirements

1. **Protocol:** Implement an **MCP stdio server** (JSON-RPC 2.0) exposing tools that:

   * Enumerate candidate windows.
   * Capture a target window (by title/class/exe).
   * On Wayland, prime and reuse **restore tokens** so subsequent runs are headless.
   * Return the screenshot **both** as an MCP **image content block** (`image/png` by default) **and** as a **ResourceLink** (`file://` temp path) for clients that prefer resource fetching.
2. **Design:** Use a single **facade** with pluggable platform backends:

   * `CaptureFacade` trait with:
     `resolve_target(...) -> TargetHandle`
     `capture_window(handle, opts) -> Image`
     `capture_display(display_id?, opts) -> Image`
     `capture_region(region, opts) -> Image`
   * Backends: `WaylandBackend` (XDG-portal), `X11Backend`, `WindowsBackend`, `MacBackend`.
3. **Targeting logic (selectors):** Support matching by **window title substring/regex**, **class/WM_CLASS/className**, and **process/exe**. Provide `list_windows` so the agent can inspect identifiers and pick the right one.
4. **Return format (tool results):**

   * **Primary:** MCP tool result with an **image content item** (PNG by default).
   * **Also include:** an MCP **ResourceLink** to a temp file (`file://.../last_screenshot.png`) with `mimeType`, `size`, and a friendly `title`.
   * Include metadata JSON: window id/title/class, backend used, geometry, timestamp, and capture time in ms.
5. **Wayland realities & strategy:**

   * Use **XDG Desktop Portal Screencast** via `ashpd`. On **first run**, show the system picker, then **persist** and **reuse** the **restore token** to skip future prompts. Use `PersistMode::ExplicitlyRevoked` where supported. Store and rotate tokens securely via **`keyring`**.
   * If restore fails or is not supported by the compositor, gracefully re-prompt; as a fallback, allow **display capture** then optional **region crop** (if the caller provides a region).
6. **Other platforms:**

   * **X11:** Enumerate toplevels via `x11rb`; match on `WM_CLASS`/`_NET_WM_NAME`. Capture via **`xcap`** (simple path) or XComposite if needed.
   * **Windows:** Enumerate with `EnumWindows` + `GetWindowTextW` + `GetClassNameW` and EXE lookup; capture via **Windows Graphics Capture** (use crate `windows-capture`).
   * **macOS:** Enumerate via `CGWindowListCopyWindowInfo`; capture with **ScreenCaptureKit** (`objc2-screen-capture-kit` or `screencapturekit`). Fall back to `CGWindowListCreateImage` for stills if SCKit unavailable.
7. **Encoding & image options:** Encode as PNG by default; support `format=webp|jpeg`, `quality`, `scale`, and `include_cursor` (best-effort depending on backend).
8. **Runtime detection:** On Linux choose backend dynamically:

   * If `$WAYLAND_DISPLAY` **and** portal available → Wayland backend.
   * Else if `$DISPLAY` → X11 backend.
   * Otherwise return a precise error.
9. **Ergonomics & UX:** idempotent and quiet by default. Log at `info` (human-readable) and `debug` (diagnostic). Time out operations sanely and surface crisp, user-actionable errors (e.g., “Portal Screencast unavailable,” “TCC Screen Recording permission denied,” “Window not found,” etc.).

---

### Tech Stack & Dependencies

* **MCP server:** `rmcp` crate (server + stdio transport) or equivalent Rust MCP SDK.
* **Common:** `image`, `thiserror`/`anyhow`, `serde`, `schemars`, `tracing`, `tempfile`, `keyring`.
* **Wayland:** `ashpd` (+ PipeWire only if consuming frames directly; prefer portal stills/streams).
* **X11:** `x11rb`, optional `xcap`.
* **Windows:** `windows` (bindings), **`windows-capture`** for WGC.
* **macOS:** `objc2-screen-capture-kit` (or `screencapturekit`) + `core-graphics`.

Use conditional compilation (`cfg(target_os = "...")`) and Cargo features: `wayland`, `x11`, `windows`, `mac`.

---

### Project Layout

```
screenshot-mcp/
  Cargo.toml
  src/
    main.rs
    mcp.rs                # server setup, tool routing, content builders
    model.rs              # request/response structs, selectors, options
    capture/
      mod.rs              # CaptureFacade + ImageBuffer helpers
      wayland.rs          # ashpd/portal impl with restore tokens
      x11.rs              # x11rb enumerate + xcap capture
      windows.rs          # EnumWindows + windows-capture
      mac.rs              # CGWindowList... + ScreenCaptureKit
    util/
      key_store.rs        # keyring-backed restore token store
      detect.rs           # choose backend on Linux
      encode.rs           # PNG/WebP/JPEG encoding
  README.md
```

---

### MCP Tools (define & implement)

1. **`list_windows`**

   * **Input:** `{ "selectorHint"?: string }`
   * **Behavior:** Return array of `{ id, title, class, owner, pid, backend }` for the active backend.
   * **Result:** JSON list (also include a small table as text for humans).
2. **`capture_window`**

   * **Input:**

     ```json
     {
       "selector": {
         "titleSubstringOrRegex"?: string,
         "class"?: string,
         "exe"?: string
       },
       "format"?: "png"|"webp"|"jpeg",
       "quality"?: 0..100,
       "scale"?: number,
       "includeCursor"?: boolean,
       "region"?: { "x":0,"y":0,"w":800,"h":600 },     // optional post-crop
       "waylandSource"?: "window"|"monitor"            // hint for Wayland
     }
     ```
   * **Behavior:** Resolve → capture → encode. On Wayland prefer restored **window** session; if not available and `waylandSource=="monitor"`, capture display and crop `region` if provided.
   * **Result:**

     * **Image content item** (PNG default).
     * **ResourceLink** to temp file (`file://.../last_screenshot.png`).
     * **Metadata JSON** block with match info + timings.
3. **`prime_wayland_consent`**

   * **Input:** `{ "source": "window"|"monitor" }`
   * **Behavior:** Start portal flow once to obtain & persist a **restore token** (PersistMode “until revoked”). Store tokens per **target name** or **scope key** (string) in `keyring`.
   * **Result:** `{ "ok": true, "persistMode": "...", "note": "token stored" }`
4. **`health_check`**

   * **Result:** `{ "platform": "...", "backend": "...", "caps": {...}, "ok": true }`

> Return **BOTH**: an image content item **and** a resource link in `capture_window`. This maximizes compatibility across MCP clients that either render inline images or prefer resource fetches.

---

### Key Implementation Details

* **Wayland (portal):**

  * Use `ashpd::desktop::screencast` to `SelectSources` (with `SourceType::Window` or `Monitor`) then `Start`. On success, extract and **store the restore token** in `keyring`. On the **next run**, pass the saved token to `SelectSources` to **restore headlessly**; always **replace** the stored token with the **new one returned** after each start (single-use tokens). If restore fails (revoked, missing window, different compositor), gracefully fall back to interactive or monitor capture.
* **X11:** Enumerate via `x11rb` (collect `_NET_WM_NAME`, `WM_CLASS`, PIDs). Resolve selector with substring/regex + fuzzy matching (use `fuzzy-matcher` or simple Levenshtein). Capture via `xcap` by window id; handle occlusion/compositor differences defensively.
* **Windows:** Use `EnumWindows` to gather `(HWND, title, class, pid, exe)`, match, then capture the chosen `HWND` via **Windows Graphics Capture** (`windows-capture`) for performance and cursor support.
* **macOS:** Use `CGWindowListCopyWindowInfo` for enumeration; resolve owner/title; capture via **ScreenCaptureKit** window capture when available (macOS 12+), else `CGWindowListCreateImage` fallback.
* **Encoding:** Convert native frames to `image::DynamicImage`, apply optional scale/region crop, then encode to `png|webp|jpeg`. Default PNG (lossless).
* **ResourceLink:** Save to a temp file with a **stable short name** (`last_screenshot.png`) per process; include `mimeType`, `size`, `title`. Also add `resources/list` + `resources/read` support if your SDK exposes it easily; otherwise returning the link in the tool result is sufficient.

---

### Error Handling & Observability

* Use a typed error enum with friendly messages and causes: `PortalUnavailable`, `PermissionDenied`, `WindowNotFound`, `CaptureFailed`, `EncodingFailed`, etc.
* Timeouts: 5s resolve, 5s capture by default (configurable via env).
* Logs: `RUST_LOG=info` by default; `debug` surfaces portal/session details (never log raw tokens).

---

### Build, Features, and CI

* `cargo` features:

  * `linux-wayland`, `linux-x11`, `windows`, `mac`.
  * On Linux, enable both `linux-wayland` and `linux-x11` by default.
* **CI matrix:** `{ ubuntu-latest, windows-latest, macos-latest }`.
* Tests:

  * Unit tests for selector matching & encoding.
  * Integration: gated “smoke” tests that **skip** on CI for portal-requiring paths; include a mock backend that generates a test image.

---

### Deliverables

* Working project per layout above.
* `README.md` with:

  * Quick start, MCP client config snippet (Cursor/Claude/Apps).
  * Permission notes: macOS TCC (Screen Recording), Wayland first-run prompt and restore behavior.
  * Examples:

    * `list_windows`
    * `capture_window` with title selector
    * Wayland `prime_wayland_consent`
* Minimal **LICENSE** (MIT or Apache-2.0).
* Example screenshots in `examples/` (non-Wayland).

---

### Acceptance Tests (manual)

1. **Wayland (KDE/GNOME/wlroots):**

   * Run `prime_wayland_consent` for `source="window"`, choose target app → confirm a stored token.
   * Close server; re-run `capture_window` with title selector → expect **no prompt**, image returned, and token rotated.
2. **X11 (fallback session or Xorg VM):**

   * `list_windows` shows app; `capture_window` returns the exact window.
3. **Windows 11:**

   * `list_windows` shows the target; `capture_window` returns an image via WGC.
4. **macOS 13+:**

   * First run: allow Screen Recording in TCC; `capture_window` works with ScreenCaptureKit; fallback still works if SCKit unavailable.

---

### Code Quality

* Rust 2021+, `clippy --all-targets --all-features -D warnings`, `rustfmt`.
* No panics on the happy path; `?` + typed errors elsewhere.
* Clear separation of protocol (MCP) and platform backends.

---

### Notes & Hints

* Prefer the **portal restore** path on Wayland; it’s the only way to avoid manual clicks later. If the compositor/portal doesn’t support restore, fall back to display capture and (optionally) crop a caller-provided region.
* Always return **both** an inline image content item **and** a **ResourceLink** so clients that don’t render inline images can still fetch the file.

**End of prompt. Build it.**

---

### References used while preparing the prompt (for your records)

* MCP resources & tool results (images + resource links): modelcontextprotocol.io — **Resources** & **Tools** concepts/specs. ([Model Context Protocol][1])
* Rust MCP server in stdio with `rmcp` (tutorial): Shuttle guide. ([Shuttle][2])
* Wayland XDG Desktop Portal Screencast: restore tokens & persist modes via `ashpd` and portal docs. ([bilelmoussaoui.github.io][3])
* Windows Graphics Capture Rust crate (`windows-capture`). ([Crates][4])
* macOS ScreenCaptureKit Rust bindings (`objc2-screen-capture-kit`). ([Docs.rs][5])

[1]: https://modelcontextprotocol.io/specification/latest?utm_source=chatgpt.com "Specification"
[2]: https://www.shuttle.dev/blog/2025/07/18/how-to-build-a-stdio-mcp-server-in-rust "How to Build a stdio MCP Server in Rust | Shuttle"
[3]: https://bilelmoussaoui.github.io/ashpd/ashpd/desktop/enum.PersistMode.html?utm_source=chatgpt.com "PersistMode in ashpd::desktop - Rust"
[4]: https://crates.io/crates/windows-capture?utm_source=chatgpt.com "windows-capture - crates.io: Rust Package Registry"
[5]: https://docs.rs/objc2-screen-capture-kit?utm_source=chatgpt.com "objc2_screen_capture_kit - Rust"
