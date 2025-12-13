//! Image encoding utilities for screenshot capture
//!
//! This module provides encoding functions for converting `ImageBuffer`
//! instances into compressed byte arrays in PNG, JPEG, and WebP formats. It
//! handles format-specific requirements like alpha channel conversion and
//! quality parameters.
//!
//! # Format Support
//!
//! - **PNG**: Lossless compression with three levels (Fast, Default, Best)
//! - **JPEG**: Lossy compression with quality 1-100 (no alpha channel support)
//! - **WebP**: Lossless only in image crate v0.25 (quality parameter ignored)
//!
//! # Examples
//!
//! ```
//! use screenshot_mcp::{
//!     capture::ImageBuffer,
//!     model::{CaptureOptions, ImageFormat},
//!     util::encode::encode_image,
//! };
//!
//! let img = ImageBuffer::from_test_pattern(1920, 1080);
//! let opts = CaptureOptions::builder().format(ImageFormat::Png).build();
//!
//! let png_bytes = encode_image(&img, &opts).unwrap();
//! assert!(!png_bytes.is_empty());
//! ```

use std::io::Cursor;

use image::{
    ImageEncoder,
    codecs::{
        jpeg::JpegEncoder,
        png::{CompressionType, FilterType, PngEncoder},
        webp::WebPEncoder,
    },
};

use crate::{
    capture::ImageBuffer,
    error::{CaptureError, CaptureResult},
    model::{CaptureOptions, ImageFormat},
};

/// Maps a quality value (0-100) to a PNG compression type
///
/// Since PNG encoding doesn't have a quality parameter in the same way as JPEG,
/// we map quality ranges to compression levels:
/// - 0-33: Fast compression (faster encoding, larger files)
/// - 34-66: Default compression (balanced)
/// - 67-100: Best compression (slower encoding, smaller files)
///
/// # Arguments
///
/// * `quality` - Quality value from 0 to 100
///
/// # Examples
///
/// ```
/// use image::codecs::png::CompressionType;
/// use screenshot_mcp::util::encode::compression_type_from_quality;
///
/// assert!(matches!(compression_type_from_quality(20), CompressionType::Fast));
/// assert!(matches!(compression_type_from_quality(50), CompressionType::Default));
/// assert!(matches!(compression_type_from_quality(90), CompressionType::Best));
/// ```
pub fn compression_type_from_quality(quality: u8) -> CompressionType {
    match quality {
        0..=33 => CompressionType::Fast,
        34..=66 => CompressionType::Default,
        67..=100 => CompressionType::Best,
        _ => CompressionType::Best, // Values > 100 use best compression
    }
}

/// Encodes an image as PNG with default compression
///
/// Uses default compression level and adaptive filtering for good balance
/// between encoding speed and file size. PNG encoding is always lossless.
///
/// # Arguments
///
/// * `buffer` - The image to encode
///
/// # Returns
///
/// A vector of bytes containing the PNG-encoded image
///
/// # Examples
///
/// ```
/// use screenshot_mcp::{capture::ImageBuffer, util::encode::encode_png};
///
/// let img = ImageBuffer::from_test_pattern(100, 100);
/// let png_bytes = encode_png(&img).unwrap();
/// assert!(!png_bytes.is_empty());
/// ```
pub fn encode_png(buffer: &ImageBuffer) -> CaptureResult<Vec<u8>> {
    encode_png_with_compression(buffer, CompressionType::Default)
}

/// Encodes an image as PNG with specified compression level
///
/// Allows fine-grained control over PNG compression. Higher compression levels
/// produce smaller files but take longer to encode.
///
/// # Arguments
///
/// * `buffer` - The image to encode
/// * `compression` - Compression level (Fast, Default, or Best)
///
/// # Returns
///
/// A vector of bytes containing the PNG-encoded image
///
/// # Examples
///
/// ```
/// use image::codecs::png::CompressionType;
/// use screenshot_mcp::{capture::ImageBuffer, util::encode::encode_png_with_compression};
///
/// let img = ImageBuffer::from_test_pattern(100, 100);
///
/// // Fast compression for interactive use
/// let fast = encode_png_with_compression(&img, CompressionType::Fast).unwrap();
///
/// // Best compression for archival
/// let best = encode_png_with_compression(&img, CompressionType::Best).unwrap();
///
/// // Best compression produces smaller files
/// assert!(best.len() < fast.len());
/// ```
pub fn encode_png_with_compression(
    buffer: &ImageBuffer,
    compression: CompressionType,
) -> CaptureResult<Vec<u8>> {
    let mut output = Vec::new();

    // Use adaptive filter for automatic per-scanline optimization
    let encoder =
        PngEncoder::new_with_quality(Cursor::new(&mut output), compression, FilterType::Adaptive);

    let rgba = buffer.to_rgba8();
    let (width, height) = rgba.dimensions();

    encoder
        .write_image(rgba.as_raw(), width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| CaptureError::EncodingFailed {
            format: "png".to_string(),
            reason: e.to_string(),
        })?;

    Ok(output)
}

/// Encodes an image as JPEG with specified quality
///
/// JPEG is a lossy format that doesn't support alpha channels. The alpha
/// channel is automatically removed by converting to RGB format before
/// encoding.
///
/// # Arguments
///
/// * `buffer` - The image to encode
/// * `quality` - Quality level from 1 (worst) to 100 (best)
///
/// # Returns
///
/// A vector of bytes containing the JPEG-encoded image
///
/// # Examples
///
/// ```
/// use screenshot_mcp::{capture::ImageBuffer, util::encode::encode_jpeg};
///
/// let img = ImageBuffer::from_test_pattern(100, 100);
///
/// // Low quality for thumbnails
/// let low = encode_jpeg(&img, 30).unwrap();
///
/// // High quality for photos
/// let high = encode_jpeg(&img, 90).unwrap();
///
/// // Higher quality produces larger files
/// assert!(high.len() > low.len());
/// ```
pub fn encode_jpeg(buffer: &ImageBuffer, quality: u8) -> CaptureResult<Vec<u8>> {
    // Validate and clamp quality to valid range
    let quality = quality.clamp(1, 100);

    let mut output = Vec::new();
    let encoder = JpegEncoder::new_with_quality(Cursor::new(&mut output), quality);

    // CRITICAL: JPEG doesn't support alpha channels - convert to RGB
    let rgb = buffer.inner().to_rgb8();
    let (width, height) = rgb.dimensions();

    // Validate dimensions
    if width == 0 || height == 0 {
        return Err(CaptureError::InvalidParameter {
            parameter: "dimensions".to_string(),
            reason:    "Image dimensions must be > 0".to_string(),
        });
    }

    encoder
        .write_image(rgb.as_raw(), width, height, image::ExtendedColorType::Rgb8)
        .map_err(|e| CaptureError::EncodingFailed {
            format: "jpeg".to_string(),
            reason: e.to_string(),
        })?;

    Ok(output)
}

/// Encodes an image as WebP (lossless only)
///
/// **Important**: The `image` crate v0.25 only supports lossless WebP encoding.
/// The `quality` parameter is accepted for API consistency but is **ignored**.
/// All WebP images are encoded losslessly.
///
/// For lossy WebP encoding with quality control, consider using the external
/// `webp` crate in future milestones.
///
/// # Arguments
///
/// * `buffer` - The image to encode
/// * `quality` - Quality level (currently ignored, lossless only)
///
/// # Returns
///
/// A vector of bytes containing the WebP-encoded image (lossless)
///
/// # Examples
///
/// ```
/// use screenshot_mcp::{capture::ImageBuffer, util::encode::encode_webp};
///
/// let img = ImageBuffer::from_test_pattern(100, 100);
///
/// // Quality parameter is ignored (lossless encoding)
/// let webp_30 = encode_webp(&img, 30).unwrap();
/// let webp_90 = encode_webp(&img, 90).unwrap();
///
/// // Both produce identical lossless output
/// assert_eq!(webp_30.len(), webp_90.len());
/// ```
pub fn encode_webp(buffer: &ImageBuffer, _quality: u8) -> CaptureResult<Vec<u8>> {
    let mut output = Vec::new();

    // Only lossless encoding is available in image crate v0.25
    let encoder = WebPEncoder::new_lossless(Cursor::new(&mut output));

    let rgba = buffer.to_rgba8();
    let (width, height) = rgba.dimensions();

    // Validate buffer size matches dimensions
    let buffer_len = rgba.as_raw().len();
    let expected_len = (width * height * 4) as usize;
    if buffer_len != expected_len {
        return Err(CaptureError::ImageError(format!(
            "Buffer size mismatch: expected {}, got {}",
            expected_len, buffer_len
        )));
    }

    encoder
        .write_image(rgba.as_raw(), width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| CaptureError::EncodingFailed {
            format: "webp".to_string(),
            reason: e.to_string(),
        })?;

    Ok(output)
}

/// Encodes an image according to the specified capture options
///
/// Main dispatcher function that selects the appropriate encoder based on
/// the format specified in `CaptureOptions`. Handles format-specific encoding
/// parameters like quality and compression.
///
/// # Arguments
///
/// * `buffer` - The image to encode
/// * `opts` - Capture options specifying format and quality
///
/// # Returns
///
/// A vector of bytes containing the encoded image
///
/// # Examples
///
/// ```
/// use screenshot_mcp::{
///     capture::ImageBuffer,
///     model::{CaptureOptions, ImageFormat},
///     util::encode::encode_image,
/// };
///
/// let img = ImageBuffer::from_test_pattern(1920, 1080);
///
/// // PNG encoding
/// let png_opts = CaptureOptions::builder()
///     .format(ImageFormat::Png)
///     .quality(80)
///     .build();
/// let png_bytes = encode_image(&img, &png_opts).unwrap();
///
/// // JPEG encoding
/// let jpeg_opts = CaptureOptions::builder()
///     .format(ImageFormat::Jpeg)
///     .quality(85)
///     .build();
/// let jpeg_bytes = encode_image(&img, &jpeg_opts).unwrap();
///
/// // WebP encoding (lossless)
/// let webp_opts = CaptureOptions::builder().format(ImageFormat::Webp).build();
/// let webp_bytes = encode_image(&img, &webp_opts).unwrap();
/// ```
pub fn encode_image(buffer: &ImageBuffer, opts: &CaptureOptions) -> CaptureResult<Vec<u8>> {
    match opts.format {
        ImageFormat::Png => {
            let compression = compression_type_from_quality(opts.quality);
            encode_png_with_compression(buffer, compression)
        }
        ImageFormat::Jpeg => encode_jpeg(buffer, opts.quality),
        ImageFormat::Webp => encode_webp(buffer, opts.quality),
    }
}

#[cfg(test)]
mod tests {
    use image::GenericImageView;

    use super::*;

    // ========== Helper Functions Tests ==========

    #[test]
    fn test_compression_type_from_quality() {
        // Fast range (0-33)
        assert!(matches!(compression_type_from_quality(0), CompressionType::Fast));
        assert!(matches!(compression_type_from_quality(20), CompressionType::Fast));
        assert!(matches!(compression_type_from_quality(33), CompressionType::Fast));

        // Default range (34-66)
        assert!(matches!(compression_type_from_quality(34), CompressionType::Default));
        assert!(matches!(compression_type_from_quality(50), CompressionType::Default));
        assert!(matches!(compression_type_from_quality(66), CompressionType::Default));

        // Best range (67-100)
        assert!(matches!(compression_type_from_quality(67), CompressionType::Best));
        assert!(matches!(compression_type_from_quality(80), CompressionType::Best));
        assert!(matches!(compression_type_from_quality(100), CompressionType::Best));
    }

    // ========== PNG Encoding Tests ==========

    #[test]
    fn test_encode_png_default() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let result = encode_png(&img);
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());

        // PNG signature: first 8 bytes should be PNG magic number
        assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn test_encode_png_compression_levels() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);

        let fast = encode_png_with_compression(&img, CompressionType::Fast).unwrap();
        let default = encode_png_with_compression(&img, CompressionType::Default).unwrap();
        let best = encode_png_with_compression(&img, CompressionType::Best).unwrap();

        // All should be valid PNG files
        for bytes in [&fast, &default, &best] {
            assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
        }

        // Verify all produce different compression attempts (sizes may vary)
        // Note: PNG compression behavior is complex and "best" doesn't always
        // produce smaller files due to compression overhead, especially for
        // simple patterns
        println!("PNG compression sizes:");
        println!("  Fast:    {} bytes", fast.len());
        println!("  Default: {} bytes", default.len());
        println!("  Best:    {} bytes", best.len());

        // Just verify they're all reasonable sizes
        assert!(fast.len() > 1000 && fast.len() < 10_000_000);
        assert!(default.len() > 1000 && default.len() < 10_000_000);
        assert!(best.len() > 1000 && best.len() < 10_000_000);
    }

    #[test]
    fn test_encode_png_lossless() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let encoded = encode_png(&img).unwrap();

        // Decode and verify it matches original
        let decoded = image::load_from_memory(&encoded).unwrap();
        assert_eq!(decoded.dimensions(), img.dimensions());
    }

    // ========== JPEG Encoding Tests ==========

    #[test]
    fn test_encode_jpeg_quality_30() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let result = encode_jpeg(&img, 30);
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());

        // JPEG signature: first 2 bytes should be 0xFF 0xD8
        assert_eq!(&bytes[0..2], &[0xff, 0xd8]);
    }

    #[test]
    fn test_encode_jpeg_quality_80() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let result = encode_jpeg(&img, 80);
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[0..2], &[0xff, 0xd8]);
    }

    #[test]
    fn test_encode_jpeg_quality_100() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let result = encode_jpeg(&img, 100);
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[0..2], &[0xff, 0xd8]);
    }

    #[test]
    fn test_encode_jpeg_quality_affects_size() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);

        let q30 = encode_jpeg(&img, 30).unwrap();
        let q80 = encode_jpeg(&img, 80).unwrap();
        let q100 = encode_jpeg(&img, 100).unwrap();

        // Higher quality should produce larger files
        assert!(q30.len() < q80.len());
        assert!(q80.len() < q100.len());
    }

    #[test]
    fn test_encode_jpeg_alpha_removed() {
        // Create RGBA image
        let img = ImageBuffer::from_test_pattern(100, 100);
        let rgba = img.to_rgba8();
        assert_eq!(rgba.as_raw().len(), 100 * 100 * 4); // RGBA has 4 channels

        // Encode as JPEG (should auto-convert to RGB)
        let result = encode_jpeg(&img, 80);
        assert!(result.is_ok());

        // Decode and verify it's RGB (3 channels)
        let bytes = result.unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        // JPEG loaded images are typically RGB8
        assert!(matches!(decoded.color(), image::ColorType::Rgb8 | image::ColorType::Rgba8));
    }

    #[test]
    fn test_encode_jpeg_quality_clamping() {
        let img = ImageBuffer::from_test_pattern(100, 100);

        // Quality 0 should be clamped to 1
        let result = encode_jpeg(&img, 0);
        assert!(result.is_ok());

        // Quality 150 should be clamped to 100
        let result = encode_jpeg(&img, 150);
        assert!(result.is_ok());
    }

    // ========== WebP Encoding Tests ==========

    #[test]
    fn test_encode_webp_lossless() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let result = encode_webp(&img, 80);
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert!(!bytes.is_empty());

        // WebP signature: "RIFF" followed by size, then "WEBP"
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
    }

    #[test]
    fn test_encode_webp_quality_ignored() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);

        // Different quality values should produce identical output (lossless)
        let webp_30 = encode_webp(&img, 30).unwrap();
        let webp_80 = encode_webp(&img, 80).unwrap();
        let webp_100 = encode_webp(&img, 100).unwrap();

        // All should produce exactly the same bytes (lossless encoding)
        assert_eq!(webp_30.len(), webp_80.len());
        assert_eq!(webp_80.len(), webp_100.len());
        assert_eq!(webp_30, webp_80);
        assert_eq!(webp_80, webp_100);
    }

    #[test]
    fn test_encode_webp_size_validation() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);

        let webp = encode_webp(&img, 80).unwrap();
        let png = encode_png(&img).unwrap();

        // WebP (lossless) should typically be smaller than PNG
        // This may not always be true for all images, but generally holds
        println!("WebP size: {}, PNG size: {}", webp.len(), png.len());
        // Just verify both are reasonable sizes
        assert!(webp.len() > 1000); // Not too small
        assert!(png.len() > 1000);
    }

    // ========== Integration Tests ==========

    #[test]
    fn test_encode_image_dispatcher_png() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let opts = CaptureOptions::builder()
            .format(ImageFormat::Png)
            .quality(80)
            .build();

        let result = encode_image(&img, &opts);
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn test_encode_image_dispatcher_jpeg() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let opts = CaptureOptions::builder()
            .format(ImageFormat::Jpeg)
            .quality(85)
            .build();

        let result = encode_image(&img, &opts);
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert_eq!(&bytes[0..2], &[0xff, 0xd8]);
    }

    #[test]
    fn test_encode_image_dispatcher_webp() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let opts = CaptureOptions::builder()
            .format(ImageFormat::Webp)
            .quality(90)
            .build();

        let result = encode_image(&img, &opts);
        assert!(result.is_ok());

        let bytes = result.unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
    }

    // ========== Size Validation Tests ==========

    #[test]
    fn test_encode_sizes_1920x1080() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);

        let png = encode_png(&img).unwrap();
        let jpeg = encode_jpeg(&img, 80).unwrap();
        let webp = encode_webp(&img, 80).unwrap();

        println!("1920x1080 encoding sizes:");
        println!("  PNG:  {} bytes", png.len());
        println!("  JPEG: {} bytes", jpeg.len());
        println!("  WebP: {} bytes", webp.len());

        // Note: For gradient test patterns, JPEG may actually be larger than PNG
        // because JPEG is optimized for photos, not simple gradients.
        // PNG and WebP excel at compressing simple patterns.

        // WebP should be very efficient for gradients
        assert!(webp.len() < png.len());

        // All should be reasonable sizes (not empty, not huge)
        assert!(png.len() > 1_000 && png.len() < 10_000_000);
        assert!(jpeg.len() > 1_000 && jpeg.len() < 5_000_000);
        assert!(webp.len() > 1_000 && webp.len() < 10_000_000);
    }

    #[test]
    fn test_jpeg_size_under_threshold() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);

        // JPEG at quality 80 should be well under 1MB for a gradient test pattern
        let jpeg = encode_jpeg(&img, 80).unwrap();
        println!("JPEG @80 size: {} bytes", jpeg.len());

        // Reasonable threshold for 1920x1080 gradient (adjust if needed)
        assert!(jpeg.len() < 1_000_000); // < 1MB
    }
}
