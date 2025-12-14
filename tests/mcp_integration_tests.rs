//! MCP Server Integration Tests
//!
//! Tests the MCP server layer end-to-end, verifying tool responses
//! match the expected structure and content.
//!
//! # Test Categories
//!
//! 1. **Headless Tests** (always run) - Use MockBackend, no display required
//! 2. **Live Windows Tests** (`#[ignore]`) - Use WindowsBackend, require
//!    desktop
//!
//! # Running Tests
//!
//! ```powershell
//! # Run all headless tests
//! cargo test --test mcp_integration_tests
//!
//! # Run live Windows tests (requires desktop environment)
//! cargo test --test mcp_integration_tests -- --ignored --nocapture
//! ```

mod common;

use common::mcp_harness::{
    ContentValidator, McpTestContext, parse_health_check, parse_window_list,
};
use screenshot_mcp::mcp::CaptureWindowParams;

// ============================================================================
// Headless Tests (MockBackend) - Always Run
// ============================================================================

/// health_check returns valid JSON with platform info
#[tokio::test]
async fn test_health_check_returns_platform_info() {
    let ctx = McpTestContext::new_with_mock();

    let result = ctx
        .health_check()
        .await
        .expect("health_check should succeed");
    assert!(!result.is_error.unwrap_or(false), "should not be an error");

    let parsed = parse_health_check(&result).expect("should parse health check JSON");
    assert!(parsed.ok, "health check should report ok=true");
    assert!(!parsed.platform.is_empty(), "platform should be detected");
    assert!(!parsed.backend.is_empty(), "backend should be detected");
}

/// list_windows returns mock window data (3 windows)
#[tokio::test]
async fn test_list_windows_returns_mock_data() {
    let ctx = McpTestContext::new_with_mock();

    let result = ctx
        .list_windows()
        .await
        .expect("list_windows should succeed");
    assert!(!result.is_error.unwrap_or(false), "should not be an error");

    let windows = parse_window_list(&result).expect("should parse window list JSON");
    assert_eq!(windows.len(), 3, "MockBackend should return 3 windows");

    // Verify window titles
    let titles: Vec<&str> = windows.iter().filter_map(|w| w["title"].as_str()).collect();
    assert!(titles.iter().any(|t| t.contains("Firefox")));
    assert!(titles.iter().any(|t| t.contains("Visual Studio Code")));
    assert!(titles.iter().any(|t| t.contains("Terminal")));
}

/// capture_window returns valid 3-part response structure
#[tokio::test]
async fn test_capture_window_returns_valid_response() {
    let ctx = McpTestContext::new_with_mock();

    let result = ctx
        .capture_window_by_title("Firefox")
        .await
        .expect("capture_window should succeed");

    let parts =
        ContentValidator::validate_capture_result(&result).expect("should have valid structure");

    // Verify PNG magic bytes
    assert!(ContentValidator::is_valid_png(&parts.image_bytes), "image should be valid PNG");

    // Verify file URI format
    assert!(parts.file_uri.starts_with("file://"), "should have file:// URI");
    assert!(parts.file_uri.contains("screenshot-"), "should reference screenshot file");

    // Verify metadata dimensions (MockBackend generates 1920x1080)
    let dims = parts.metadata["dimensions"]
        .as_array()
        .expect("should have dimensions");
    assert_eq!(dims[0].as_u64(), Some(1920), "width should be 1920");
    assert_eq!(dims[1].as_u64(), Some(1080), "height should be 1080");
}

/// capture_window creates temp file that persists
#[tokio::test]
async fn test_capture_window_creates_temp_file() {
    let ctx = McpTestContext::new_with_mock();

    assert_eq!(ctx.temp_file_count(), 0, "should start with no temp files");

    let result = ctx
        .capture_window_by_title("Firefox")
        .await
        .expect("capture should succeed");

    assert_eq!(ctx.temp_file_count(), 1, "should have 1 temp file after capture");

    // Verify file exists at the URI path
    let uri = ContentValidator::validate_file_uri(&result).expect("should have file URI");

    // Parse file:// URI to path
    let path = uri
        .strip_prefix("file:///")
        .or_else(|| uri.strip_prefix("file://"))
        .expect("should have file:// prefix");

    // On Windows, convert forward slashes
    #[cfg(target_os = "windows")]
    let path = path.replace('/', "\\");

    assert!(std::path::Path::new(&path).exists(), "temp file should exist at: {}", path);
}

/// capture_window produces valid PNG with magic bytes
#[tokio::test]
async fn test_capture_window_png_magic_bytes() {
    let ctx = McpTestContext::new_with_mock();

    let result = ctx
        .capture_window_by_title("Visual Studio Code")
        .await
        .expect("capture should succeed");

    let image_bytes =
        ContentValidator::validate_base64_image(&result, "image/png").expect("should decode image");

    // PNG magic bytes: 89 50 4E 47 0D 0A 1A 0A
    assert!(image_bytes.len() >= 8, "image should have at least 8 bytes");
    assert_eq!(image_bytes[0], 0x89, "PNG signature byte 0");
    assert_eq!(image_bytes[1], 0x50, "PNG signature byte 1 (P)");
    assert_eq!(image_bytes[2], 0x4e, "PNG signature byte 2 (N)");
    assert_eq!(image_bytes[3], 0x47, "PNG signature byte 3 (G)");
    assert_eq!(image_bytes[4], 0x0d, "PNG signature byte 4");
    assert_eq!(image_bytes[5], 0x0a, "PNG signature byte 5");
    assert_eq!(image_bytes[6], 0x1a, "PNG signature byte 6");
    assert_eq!(image_bytes[7], 0x0a, "PNG signature byte 7");
}

/// capture_window with missing selector fails with appropriate error
#[tokio::test]
async fn test_capture_window_missing_selector_fails() {
    let ctx = McpTestContext::new_with_mock();

    let result = ctx
        .capture_window(CaptureWindowParams {
            title_substring_or_regex: None,
            class: None,
            exe: None,
        })
        .await;

    assert!(result.is_err(), "should fail with no selector");

    let error = result.unwrap_err();
    let error_msg = format!("{:?}", error);
    assert!(error_msg.contains("must be specified"), "error should mention missing selector");
}

/// capture_window with nonexistent window fails
#[tokio::test]
async fn test_capture_window_not_found_fails() {
    let ctx = McpTestContext::new_with_mock();

    let result = ctx.capture_window_by_title("NonexistentWindow99999").await;

    assert!(result.is_err(), "should fail for nonexistent window");
}

/// capture_window by class name works
#[tokio::test]
async fn test_capture_window_by_class() {
    let ctx = McpTestContext::new_with_mock();

    let result = ctx
        .capture_window_by_class("Navigator")
        .await
        .expect("should find Firefox by class");

    assert_eq!(result.content.len(), 3, "should have 3 content items");
}

/// capture_window by exe name works
#[tokio::test]
async fn test_capture_window_by_exe() {
    let ctx = McpTestContext::new_with_mock();

    let result = ctx
        .capture_window_by_exe("code")
        .await
        .expect("should find VSCode by exe");

    assert_eq!(result.content.len(), 3, "should have 3 content items");
}

/// Multiple captures create unique temp files
#[tokio::test]
async fn test_multiple_captures_create_unique_files() {
    let ctx = McpTestContext::new_with_mock();

    let _r1 = ctx.capture_window_by_title("Firefox").await.unwrap();
    let _r2 = ctx
        .capture_window_by_title("Visual Studio Code")
        .await
        .unwrap();
    let _r3 = ctx.capture_window_by_title("Terminal").await.unwrap();

    assert_eq!(ctx.temp_file_count(), 3, "should have 3 temp files");

    let files = ctx.temp_file_paths();
    assert_eq!(files.len(), 3);

    // All should be unique
    assert_ne!(files[0], files[1]);
    assert_ne!(files[1], files[2]);
    assert_ne!(files[0], files[2]);

    // All should exist
    for file in &files {
        assert!(file.exists(), "temp file should exist: {:?}", file);
    }
}

// ============================================================================
// Error Injection Tests
// ============================================================================

/// Test error handling with injected permission error
#[tokio::test]
async fn test_capture_with_injected_permission_error() {
    use screenshot_mcp::{capture::MockBackend, error::CaptureError, model::BackendType};

    let mock = MockBackend::new().with_error(CaptureError::PermissionDenied {
        platform: "test".to_string(),
        backend: BackendType::None,
    });
    let ctx = McpTestContext::new_with_configured_mock(mock);

    let result = ctx.list_windows().await;
    assert!(result.is_err(), "should propagate permission error");
}

/// Test error handling with injected timeout error
#[tokio::test]
async fn test_capture_with_injected_timeout_error() {
    use screenshot_mcp::{capture::MockBackend, error::CaptureError};

    let mock = MockBackend::new().with_error(CaptureError::CaptureTimeout { duration_ms: 5000 });
    let ctx = McpTestContext::new_with_configured_mock(mock);

    let result = ctx.capture_window_by_title("Firefox").await;
    assert!(result.is_err(), "should propagate timeout error");
}

/// Test error handling with injected window not found error
#[tokio::test]
async fn test_capture_with_injected_window_not_found() {
    use screenshot_mcp::{capture::MockBackend, error::CaptureError, model::WindowSelector};

    let mock = MockBackend::new().with_error(CaptureError::WindowNotFound {
        selector: WindowSelector::by_title("test"),
    });
    let ctx = McpTestContext::new_with_configured_mock(mock);

    let result = ctx.capture_window_by_title("Firefox").await;
    assert!(result.is_err(), "should propagate window not found error");
}

// ============================================================================
// Live Windows Tests (requires desktop environment)
// ============================================================================

#[cfg(target_os = "windows")]
mod live_windows_tests {
    use super::*;

    /// Live test: health_check on real Windows system
    #[tokio::test]
    #[ignore = "requires Windows desktop environment"]
    async fn test_live_health_check_windows() {
        let ctx = McpTestContext::new_with_windows_backend();

        let result = ctx
            .health_check()
            .await
            .expect("health_check should succeed");

        let parsed = parse_health_check(&result).expect("should parse");

        assert_eq!(parsed.platform, "windows", "platform should be windows");
        assert_eq!(parsed.backend, "windows", "backend should be windows");
        assert!(parsed.ok, "should be ok");
    }

    /// Live test: list_windows returns real windows
    #[tokio::test]
    #[ignore = "requires Windows desktop environment"]
    async fn test_live_list_windows_windows() {
        let ctx = McpTestContext::new_with_windows_backend();

        let result = ctx
            .list_windows()
            .await
            .expect("list_windows should succeed");

        let windows = parse_window_list(&result).expect("should parse");

        assert!(!windows.is_empty(), "should have at least one window");
        println!("Found {} windows", windows.len());

        // Print first few windows for debugging
        for (i, w) in windows.iter().take(5).enumerate() {
            println!(
                "  [{}] {} (class: {}, exe: {})",
                i,
                w["title"].as_str().unwrap_or("?"),
                w["class"].as_str().unwrap_or("?"),
                w["owner"].as_str().unwrap_or("?")
            );
        }
    }

    /// Live test: capture real window via MCP server
    #[tokio::test]
    #[ignore = "requires Windows desktop environment"]
    async fn test_live_capture_window_windows() {
        let ctx = McpTestContext::new_with_windows_backend();

        // List windows first to find a target
        let list_result = ctx
            .list_windows()
            .await
            .expect("list_windows should succeed");
        let windows = parse_window_list(&list_result).expect("should parse windows");

        assert!(!windows.is_empty(), "need at least one window to capture");

        // Find a suitable window (prefer known apps)
        let target = windows
            .iter()
            .find(|w| {
                let title = w["title"].as_str().unwrap_or("");
                title.contains("Visual Studio Code")
                    || title.contains("Firefox")
                    || title.contains("Chrome")
                    || title.contains("Explorer")
            })
            .or_else(|| windows.first())
            .expect("should have at least one window");

        let title = target["title"].as_str().expect("window should have title");
        println!("Capturing window: {}", title);

        let result = ctx
            .capture_window_by_title(title)
            .await
            .expect("capture_window should succeed");

        let parts =
            ContentValidator::validate_capture_result(&result).expect("should have valid result");

        // Verify real image content (should be substantial)
        assert!(
            parts.image_bytes.len() > 1000,
            "real screenshot should be > 1KB, got {} bytes",
            parts.image_bytes.len()
        );

        // Verify PNG format
        assert!(ContentValidator::is_valid_png(&parts.image_bytes), "should be valid PNG");

        println!(
            "Captured {} bytes, dimensions {:?}",
            parts.image_bytes.len(),
            parts.metadata["dimensions"]
        );
    }

    /// Live test: capture display (full screen)
    #[tokio::test]
    #[ignore = "requires Windows desktop environment"]
    async fn test_live_capture_display_windows() {
        // Note: capture_display is not exposed via MCP tools currently,
        // but we can still test the backend directly if needed.
        // This test is a placeholder for future display capture support.
        println!("Display capture via MCP not yet implemented");
    }

    /// Live test: capture Cursor editor window
    ///
    /// Cursor is a VS Code fork, so window title typically contains "Cursor"
    /// and the executable is "Cursor.exe" or "cursor".
    #[tokio::test]
    #[ignore = "requires Windows desktop environment with Cursor running"]
    async fn test_live_capture_cursor_editor() {
        let ctx = McpTestContext::new_with_windows_backend();

        // List windows to find Cursor
        let list_result = ctx
            .list_windows()
            .await
            .expect("list_windows should succeed");
        let windows = parse_window_list(&list_result).expect("should parse windows");

        // Find Cursor window (title usually contains "Cursor" or exe is "Cursor")
        let cursor_window = windows.iter().find(|w| {
            let title = w["title"].as_str().unwrap_or("");
            let owner = w["owner"].as_str().unwrap_or("");
            title.contains("Cursor") || owner.to_lowercase().contains("cursor")
        });

        if cursor_window.is_none() {
            println!("Available windows:");
            for w in windows.iter().take(10) {
                println!(
                    "  - {} (owner: {})",
                    w["title"].as_str().unwrap_or("?"),
                    w["owner"].as_str().unwrap_or("?")
                );
            }
            panic!("Cursor editor not found. Make sure Cursor is running.");
        }

        let cursor = cursor_window.unwrap();
        let title = cursor["title"].as_str().expect("should have title");
        println!("Found Cursor window: {}", title);

        // Capture the Cursor window
        let result = ctx
            .capture_window_by_title("Cursor")
            .await
            .expect("capture_window should succeed for Cursor");

        let parts =
            ContentValidator::validate_capture_result(&result).expect("should have valid result");

        // Verify we got a real screenshot
        assert!(ContentValidator::is_valid_png(&parts.image_bytes), "should be valid PNG");
        assert!(
            parts.image_bytes.len() > 10_000,
            "Cursor screenshot should be > 10KB, got {} bytes",
            parts.image_bytes.len()
        );

        // Log dimensions
        let dims = parts.metadata["dimensions"]
            .as_array()
            .expect("should have dimensions");
        println!(
            "Captured Cursor: {}x{} pixels, {} bytes",
            dims[0],
            dims[1],
            parts.image_bytes.len()
        );

        // Save to test output for visual verification
        let output_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_output");
        std::fs::create_dir_all(&output_dir).ok();
        let output_path = output_dir.join("cursor_capture.png");
        std::fs::write(&output_path, &parts.image_bytes).expect("should write file");
        println!("Saved to: {}", output_path.display());
    }
}
