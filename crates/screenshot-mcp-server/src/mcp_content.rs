//! MCP content builders for screenshot capture results
//!
//! This module provides utilities for converting screenshot data into MCP
//! protocol responses. It creates dual-format output combining inline image
//! content (for immediate preview) and file resource links (for persistent
//! access).
//!
//! # Examples
//!
//! ```
//! use std::path::PathBuf;
//!
//! use screenshot_core::model::{CaptureOptions, ImageFormat};
//! use screenshot_mcp_server::mcp_content::build_capture_result;
//!
//! let image_data = vec![0u8; 100]; // Mock image data
//! let file_path = PathBuf::from("/tmp/screenshot-12345.png");
//! let opts = CaptureOptions::builder().format(ImageFormat::Png).build();
//! let dimensions = (1920, 1080);
//!
//! let result = build_capture_result(&image_data, &file_path, &opts, dimensions);
//! assert!(!result.is_error.unwrap_or(false));
//! assert_eq!(result.content.len(), 3); // Image + ResourceLink + Metadata
//! ```

use std::path::Path;

use base64::{Engine, engine::general_purpose::STANDARD};
use rmcp::model::{CallToolResult, Content};
use screenshot_core::model::CaptureOptions;

/// Builds MCP image content from raw image bytes
///
/// Creates inline image content by base64-encoding the raw bytes and wrapping
/// them in an MCP `Content::Image` structure. This allows MCP clients to
/// display the image directly without fetching an external file.
///
/// # Arguments
///
/// * `data` - Raw image bytes (PNG, JPEG, or WebP encoded)
/// * `mime_type` - MIME type string (e.g., "image/png", "image/jpeg")
///
/// # Returns
///
/// An MCP `Content` object containing the base64-encoded image
///
/// # Examples
///
/// ```
/// use screenshot_mcp_server::mcp_content::build_image_content;
///
/// // Mock PNG data (8-byte PNG signature + minimal content)
/// let png_data = vec![137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13];
/// let content = build_image_content(&png_data, "image/png");
///
/// // Content should be created successfully
/// assert!(content.as_image().is_some());
/// ```
pub fn build_image_content(data: &[u8], mime_type: &str) -> Content {
    // Base64 encode the raw image bytes
    let encoded = STANDARD.encode(data);

    // Create MCP image content with encoded data
    Content::image(encoded, mime_type)
}

/// Builds MCP resource reference from a file path
///
/// Creates a text content block with structured information about the
/// screenshot file, including:
/// - A `file://` URI for the file path
/// - The filename
/// - A descriptive title with ISO 8601 timestamp
/// - MIME type and file size metadata
///
/// Note: This creates a text-based resource reference since the rmcp 0.3.2
/// resource module is private. Future versions may use native resource links.
///
/// # Arguments
///
/// * `path` - Path to the screenshot file
/// * `mime_type` - MIME type string (e.g., "image/png")
/// * `size` - File size in bytes
///
/// # Returns
///
/// An MCP `Content` object containing resource information
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
///
/// use screenshot_mcp_server::mcp_content::build_resource_link;
///
/// let path = PathBuf::from("/tmp/screenshot-12345.png");
/// let content = build_resource_link(&path, "image/png", 12345);
///
/// // Content should be text with file information
/// assert!(content.as_text().is_some());
/// ```
pub fn build_resource_link(path: &Path, mime_type: &str, size: u64) -> Content {
    // Convert path to file:// URI
    let path_str = path.to_string_lossy();

    // Format as file:// URI - use platform-appropriate separator
    #[cfg(target_os = "windows")]
    let uri = format!("file:///{}", path_str.replace('\\', "/"));

    #[cfg(not(target_os = "windows"))]
    let uri = format!("file://{}", path_str);

    // Extract filename
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("screenshot.png");

    // Generate timestamp
    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    // Format as structured markdown
    let content_text = format!(
        "## Screenshot File Reference\n\n**File:** [{}]({})\n**Timestamp:** {}\n**Size:** {} \
         bytes\n**MIME Type:** {}\n\n_The screenshot has been saved to the path above._",
        filename, uri, timestamp, size, mime_type
    );

    Content::text(content_text)
}

/// Builds a complete MCP capture result with dual-format output
///
/// Creates a comprehensive `CallToolResult` containing:
/// 1. Inline image content (base64-encoded for immediate preview)
/// 2. Resource link (file:// URI for persistent access)
/// 3. Metadata (JSON with dimensions, format, and file size)
///
/// This dual-format approach allows MCP clients to:
/// - Display the screenshot immediately via the inline image
/// - Access the file later via the persistent file link
///
/// # Arguments
///
/// * `image_data` - Raw encoded image bytes (PNG/JPEG/WebP)
/// * `file_path` - Path where the screenshot was saved
/// * `opts` - Capture options (contains format information)
/// * `dimensions` - Image dimensions as (width, height)
///
/// # Returns
///
/// An MCP `CallToolResult` containing all content and metadata
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
///
/// use screenshot_core::model::{CaptureOptions, ImageFormat};
/// use screenshot_mcp_server::mcp_content::build_capture_result;
///
/// let image_data = vec![137, 80, 78, 71, 13, 10, 26, 10]; // PNG signature
/// let file_path = PathBuf::from("/tmp/screenshot.png");
/// let opts = CaptureOptions::builder()
///     .format(ImageFormat::Png)
///     .quality(80)
///     .build();
/// let dimensions = (1920, 1080);
///
/// let result = build_capture_result(&image_data, &file_path, &opts, dimensions);
///
/// // Result should be successful
/// assert!(!result.is_error.unwrap_or(false));
///
/// // Should contain image, resource link, and metadata
/// assert!(result.content.len() >= 2);
/// ```
pub fn build_capture_result(
    image_data: &[u8],
    file_path: &Path,
    opts: &CaptureOptions,
    dimensions: (u32, u32),
) -> CallToolResult {
    // Get MIME type from format
    let mime_type = opts.format.mime_type();
    let file_size = image_data.len() as u64;

    // Build inline image content (for immediate preview)
    let image_content = build_image_content(image_data, mime_type);

    // Build resource link (for persistent file access)
    let resource_link = build_resource_link(file_path, mime_type, file_size);

    // Build metadata as JSON text content
    let metadata = serde_json::json!({
        "dimensions": [dimensions.0, dimensions.1],
        "format": opts.format.to_string(),
        "size_bytes": file_size,
        "quality": opts.quality,
        "scale": opts.scale,
        "file_path": file_path.to_string_lossy(),
    });

    let metadata_str = serde_json::to_string_pretty(&metadata)
        .unwrap_or_else(|_| r#"{"error": "Failed to serialize metadata"}"#.to_string());
    let metadata_content =
        Content::text(format!("## Capture Metadata\n\n```json\n{}\n```", metadata_str));

    // Combine all content into success result
    CallToolResult::success(vec![image_content, resource_link, metadata_content])
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use screenshot_core::model::ImageFormat;

    // ========== build_image_content Tests ==========

    #[test]
    fn test_build_image_content_png() {
        // Mock PNG data (PNG signature)
        let png_data = vec![137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13];
        let content = build_image_content(&png_data, "image/png");

        // Should be image content
        assert!(content.as_image().is_some());
    }

    #[test]
    fn test_build_image_content_base64_encoding() {
        let data = b"test data";
        let content = build_image_content(data, "image/png");

        // Should contain base64-encoded data
        let image = content.as_image().unwrap();
        let decoded = STANDARD.decode(&image.data).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_build_image_content_empty_data() {
        let content = build_image_content(&[], "image/png");
        assert!(content.as_image().is_some());
    }

    #[test]
    fn test_build_image_content_different_mime_types() {
        let data = b"test";

        let png = build_image_content(data, "image/png");
        assert!(png.as_image().is_some());

        let jpeg = build_image_content(data, "image/jpeg");
        assert!(jpeg.as_image().is_some());

        let webp = build_image_content(data, "image/webp");
        assert!(webp.as_image().is_some());
    }

    // ========== build_resource_link Tests ==========

    #[test]
    fn test_build_resource_link_unix_path() {
        let path = PathBuf::from("/tmp/screenshot-12345.png");
        let content = build_resource_link(&path, "image/png", 12345);

        let text = content.as_text().unwrap();

        // Should contain file:// URI
        #[cfg(not(target_os = "windows"))]
        assert!(text.text.contains("file:///tmp/"));

        // Should contain filename
        assert!(text.text.contains("screenshot-12345.png"));

        // Should contain MIME type
        assert!(text.text.contains("image/png"));

        // Should contain size
        assert!(text.text.contains("12345"));

        // Should contain timestamp marker (ISO 8601 format has 'T')
        assert!(text.text.contains("T"));

        // Should be formatted as markdown
        assert!(text.text.contains("##"));
    }

    #[test]
    fn test_build_resource_link_filename_extraction() {
        let path = PathBuf::from("/var/tmp/test-screenshot.webp");
        let content = build_resource_link(&path, "image/webp", 54321);

        let text = content.as_text().unwrap();
        assert!(text.text.contains("test-screenshot.webp"));
    }

    #[test]
    fn test_build_resource_link_mime_types() {
        let path = PathBuf::from("/tmp/test.png");

        let png = build_resource_link(&path, "image/png", 100);
        assert!(png.as_text().unwrap().text.contains("image/png"));

        let jpeg = build_resource_link(&path, "image/jpeg", 100);
        assert!(jpeg.as_text().unwrap().text.contains("image/jpeg"));

        let webp = build_resource_link(&path, "image/webp", 100);
        assert!(webp.as_text().unwrap().text.contains("image/webp"));
    }

    // ========== build_capture_result Tests ==========

    #[test]
    fn test_build_capture_result_structure() {
        let image_data = vec![137, 80, 78, 71, 13, 10, 26, 10]; // PNG signature
        let file_path = PathBuf::from("/tmp/screenshot.png");
        let opts = CaptureOptions::builder()
            .format(ImageFormat::Png)
            .quality(80)
            .build();
        let dimensions = (1920, 1080);

        let result = build_capture_result(&image_data, &file_path, &opts, dimensions);

        // Should be successful
        assert!(!result.is_error.unwrap_or(false));

        // Should have 3 content items: image + file reference + metadata
        assert_eq!(result.content.len(), 3);

        // First should be image
        assert!(result.content[0].as_image().is_some());

        // Second should be text (file reference)
        assert!(result.content[1].as_text().is_some());

        // Third should be text (metadata)
        assert!(result.content[2].as_text().is_some());
    }

    #[test]
    fn test_build_capture_result_metadata_contains_dimensions() {
        let image_data = vec![0u8; 100];
        let file_path = PathBuf::from("/tmp/test.png");
        let opts = CaptureOptions::default();
        let dimensions = (3840, 2160); // 4K

        let result = build_capture_result(&image_data, &file_path, &opts, dimensions);

        // Extract metadata text
        let metadata_text = result.content[2].as_text().unwrap();

        // Should contain dimensions
        assert!(metadata_text.text.contains("3840"));
        assert!(metadata_text.text.contains("2160"));
    }

    #[test]
    fn test_build_capture_result_metadata_contains_format() {
        let image_data = vec![0u8; 100];
        let file_path = PathBuf::from("/tmp/test.webp");
        let opts = CaptureOptions::builder().format(ImageFormat::Webp).build();

        let result = build_capture_result(&image_data, &file_path, &opts, (1920, 1080));

        let metadata_text = result.content[2].as_text().unwrap();
        assert!(metadata_text.text.contains("webp"));
    }

    #[test]
    fn test_build_capture_result_metadata_contains_size() {
        let image_data = vec![0u8; 12345];
        let file_path = PathBuf::from("/tmp/test.png");
        let opts = CaptureOptions::default();

        let result = build_capture_result(&image_data, &file_path, &opts, (1920, 1080));

        let metadata_text = result.content[2].as_text().unwrap();
        assert!(metadata_text.text.contains("12345"));
    }

    #[test]
    fn test_build_capture_result_mime_type_matches_format() {
        let image_data = vec![0u8; 100];
        let file_path = PathBuf::from("/tmp/test.jpg");

        // Test PNG
        let opts_png = CaptureOptions::builder().format(ImageFormat::Png).build();
        let result_png = build_capture_result(&image_data, &file_path, &opts_png, (1920, 1080));
        let image_png = result_png.content[0].as_image().unwrap();
        assert_eq!(image_png.mime_type, "image/png");

        // Test JPEG
        let opts_jpeg = CaptureOptions::builder().format(ImageFormat::Jpeg).build();
        let result_jpeg = build_capture_result(&image_data, &file_path, &opts_jpeg, (1920, 1080));
        let image_jpeg = result_jpeg.content[0].as_image().unwrap();
        assert_eq!(image_jpeg.mime_type, "image/jpeg");

        // Test WebP
        let opts_webp = CaptureOptions::builder().format(ImageFormat::Webp).build();
        let result_webp = build_capture_result(&image_data, &file_path, &opts_webp, (1920, 1080));
        let image_webp = result_webp.content[0].as_image().unwrap();
        assert_eq!(image_webp.mime_type, "image/webp");
    }

    #[test]
    fn test_build_capture_result_with_quality_and_scale() {
        let image_data = vec![0u8; 100];
        let file_path = PathBuf::from("/tmp/test.png");
        let opts = CaptureOptions::builder()
            .format(ImageFormat::Jpeg)
            .quality(90)
            .scale(0.5)
            .build();

        let result = build_capture_result(&image_data, &file_path, &opts, (1920, 1080));

        let metadata_text = result.content[2].as_text().unwrap();

        // Should contain quality and scale
        assert!(metadata_text.text.contains("90"));
        assert!(metadata_text.text.contains("0.5"));
    }
}
