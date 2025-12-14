//! Image buffer wrapper for screenshot data
//!
//! This module provides an `ImageBuffer` wrapper around `image::DynamicImage`
//! with methods for common image transformations and conversions used in
//! screenshot capture workflows.
//!
//! # Examples
//!
//! ```
//! use screenshot_mcp::{capture::ImageBuffer, model::Region};
//!
//! // Create a test pattern
//! let img = ImageBuffer::from_test_pattern(1920, 1080);
//!
//! // Scale down to 50%
//! let scaled = img.scale(0.5).unwrap();
//! assert_eq!(scaled.dimensions(), (960, 540));
//!
//! // Crop a region (fits within 960x540)
//! let region = Region::new(100, 100, 800, 400);
//! let cropped = scaled.crop(region).unwrap();
//! assert_eq!(cropped.dimensions(), (800, 400));
//! ```

use image::GenericImageView;

use crate::{
    error::{CaptureError, CaptureResult},
    model::Region,
};

/// Wrapper around `image::DynamicImage` with transformation methods
///
/// Provides a simplified interface for common image operations needed
/// for screenshot capture, including scaling, cropping, and format conversion.
///
/// All transformation methods return new `ImageBuffer` instances, leaving
/// the original unchanged (immutable operations).
#[derive(Clone, Debug)]
pub struct ImageBuffer {
    inner: image::DynamicImage,
}

impl ImageBuffer {
    /// Creates a new ImageBuffer from a DynamicImage
    ///
    /// # Examples
    ///
    /// ```
    /// use image::DynamicImage;
    /// use screenshot_mcp::capture::ImageBuffer;
    ///
    /// let dynamic = DynamicImage::new_rgb8(100, 100);
    /// let buffer = ImageBuffer::new(dynamic);
    /// ```
    pub fn new(image: image::DynamicImage) -> Self {
        Self { inner: image }
    }

    /// Scales the image by the given factor
    ///
    /// Uses Lanczos3 filtering for high-quality scaling. The scale factor
    /// must be between 0.1 and 2.0. Values outside this range will be clamped.
    ///
    /// # Arguments
    ///
    /// * `factor` - Scale factor (0.1 = 10% size, 1.0 = original, 2.0 = 200%
    ///   size)
    ///
    /// # Returns
    ///
    /// A new `ImageBuffer` with scaled dimensions, or an error if scaling
    /// fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::capture::ImageBuffer;
    ///
    /// let img = ImageBuffer::from_test_pattern(1920, 1080);
    ///
    /// // Scale to 50%
    /// let half = img.scale(0.5).unwrap();
    /// assert_eq!(half.dimensions(), (960, 540));
    ///
    /// // Scale to 200%
    /// let double = img.scale(2.0).unwrap();
    /// assert_eq!(double.dimensions(), (3840, 2160));
    /// ```
    pub fn scale(&self, factor: f32) -> CaptureResult<Self> {
        // Clamp factor to valid range
        let factor = factor.clamp(0.1, 2.0);

        let (width, height) = self.dimensions();
        let new_width = ((width as f32) * factor) as u32;
        let new_height = ((height as f32) * factor) as u32;

        // Short-circuit if dimensions are unchanged
        if new_width == width && new_height == height {
            return Ok(self.clone());
        }

        // Ensure dimensions are at least 1x1
        let new_width = new_width.max(1);
        let new_height = new_height.max(1);

        let scaled =
            self.inner
                .resize(new_width, new_height, image::imageops::FilterType::Lanczos3);

        Ok(Self::new(scaled))
    }

    /// Crops the image to the specified region
    ///
    /// The region must be within the image bounds, otherwise an error is
    /// returned.
    ///
    /// # Arguments
    ///
    /// * `region` - The rectangular region to crop
    ///
    /// # Returns
    ///
    /// A new `ImageBuffer` containing only the cropped region, or an error
    /// if the region is out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::{capture::ImageBuffer, model::Region};
    ///
    /// let img = ImageBuffer::from_test_pattern(1920, 1080);
    ///
    /// // Crop to a 800x600 region starting at (100, 100)
    /// let region = Region::new(100, 100, 800, 600);
    /// let cropped = img.crop(region).unwrap();
    /// assert_eq!(cropped.dimensions(), (800, 600));
    /// ```
    pub fn crop(&self, region: Region) -> CaptureResult<Self> {
        let (img_width, img_height) = self.dimensions();

        // Validate region bounds
        if region.x >= img_width || region.y >= img_height {
            return Err(CaptureError::InvalidParameter {
                parameter: "region".to_string(),
                reason: format!(
                    "Region origin ({}, {}) is outside image bounds ({}x{})",
                    region.x, region.y, img_width, img_height
                ),
            });
        }

        if region.x + region.width > img_width || region.y + region.height > img_height {
            return Err(CaptureError::InvalidParameter {
                parameter: "region".to_string(),
                reason: format!(
                    "Region ({}x{} at {},{}) extends beyond image bounds ({}x{})",
                    region.width, region.height, region.x, region.y, img_width, img_height
                ),
            });
        }

        // Clone the image and crop it
        let mut cloned = self.inner.clone();
        let cropped = cloned.crop(region.x, region.y, region.width, region.height);

        Ok(Self::new(cropped))
    }

    /// Returns the dimensions of the image as (width, height)
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::capture::ImageBuffer;
    ///
    /// let img = ImageBuffer::from_test_pattern(1920, 1080);
    /// assert_eq!(img.dimensions(), (1920, 1080));
    /// ```
    pub fn dimensions(&self) -> (u32, u32) {
        self.inner.dimensions()
    }

    /// Returns the image width in pixels
    pub fn width(&self) -> u32 {
        self.dimensions().0
    }

    /// Returns the image height in pixels
    pub fn height(&self) -> u32 {
        self.dimensions().1
    }

    /// Converts the image to RGBA8 format
    ///
    /// Returns an `ImageBuffer<Rgba<u8>, Vec<u8>>` which can be used for
    /// further processing or encoding.
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::capture::ImageBuffer;
    ///
    /// let img = ImageBuffer::from_test_pattern(100, 100);
    /// let rgba = img.to_rgba8();
    /// assert_eq!(rgba.dimensions(), (100, 100));
    /// ```
    pub fn to_rgba8(&self) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
        self.inner.to_rgba8()
    }

    /// Returns a reference to the raw pixel data as bytes
    ///
    /// The format of the bytes depends on the underlying image format.
    /// For predictable byte layout, convert to RGBA8 first using `to_rgba8()`.
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::capture::ImageBuffer;
    ///
    /// let img = ImageBuffer::from_test_pattern(100, 100);
    /// let bytes = img.as_bytes();
    /// assert!(bytes.len() > 0);
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        self.inner.as_bytes()
    }

    /// Creates a test pattern image with the specified dimensions
    ///
    /// Generates a colored gradient pattern useful for testing without
    /// requiring a real capture backend. The pattern is a vertical gradient
    /// from blue (top) to cyan (bottom).
    ///
    /// # Arguments
    ///
    /// * `width` - Width of the test pattern in pixels
    /// * `height` - Height of the test pattern in pixels
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::capture::ImageBuffer;
    ///
    /// // Create a 1920x1080 test pattern
    /// let img = ImageBuffer::from_test_pattern(1920, 1080);
    /// assert_eq!(img.dimensions(), (1920, 1080));
    /// ```
    pub fn from_test_pattern(width: u32, height: u32) -> Self {
        use image::{ImageBuffer as ImgBuf, Rgba};

        // Create a vertical gradient from blue to cyan
        let start_color = Rgba([0u8, 0u8, 255u8, 255u8]); // Blue
        let end_color = Rgba([0u8, 255u8, 255u8, 255u8]); // Cyan

        let img = ImgBuf::from_fn(width, height, |_x, y| {
            let ratio = y as f32 / height.max(1) as f32;
            Rgba([
                (start_color[0] as f32 * (1.0 - ratio) + end_color[0] as f32 * ratio) as u8,
                (start_color[1] as f32 * (1.0 - ratio) + end_color[1] as f32 * ratio) as u8,
                (start_color[2] as f32 * (1.0 - ratio) + end_color[2] as f32 * ratio) as u8,
                255,
            ])
        });

        Self::new(image::DynamicImage::ImageRgba8(img))
    }

    /// Returns a reference to the inner DynamicImage
    ///
    /// This allows direct access to the underlying `image::DynamicImage`
    /// for operations not provided by this wrapper.
    pub fn inner(&self) -> &image::DynamicImage {
        &self.inner
    }

    /// Consumes self and returns the inner DynamicImage
    ///
    /// Useful when you need ownership of the underlying image.
    pub fn into_inner(self) -> image::DynamicImage {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_from_dynamic_image() {
        let dynamic = image::DynamicImage::new_rgb8(100, 100);
        let buffer = ImageBuffer::new(dynamic);
        assert_eq!(buffer.dimensions(), (100, 100));
    }

    #[test]
    fn test_dimensions() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);
        assert_eq!(img.dimensions(), (1920, 1080));

        let img = ImageBuffer::from_test_pattern(640, 480);
        assert_eq!(img.dimensions(), (640, 480));
    }

    #[test]
    fn test_scale_valid_factors() {
        let img = ImageBuffer::from_test_pattern(1000, 1000);

        // Scale to 50%
        let half = img.scale(0.5).unwrap();
        assert_eq!(half.dimensions(), (500, 500));

        // Scale to 100% (no change)
        let same = img.scale(1.0).unwrap();
        assert_eq!(same.dimensions(), (1000, 1000));

        // Scale to 150%
        let larger = img.scale(1.5).unwrap();
        assert_eq!(larger.dimensions(), (1500, 1500));

        // Scale to 200%
        let double = img.scale(2.0).unwrap();
        assert_eq!(double.dimensions(), (2000, 2000));
    }

    #[test]
    fn test_scale_min_factor() {
        let img = ImageBuffer::from_test_pattern(1000, 1000);

        // Minimum scale factor (0.1 = 10%)
        let tiny = img.scale(0.1).unwrap();
        assert_eq!(tiny.dimensions(), (100, 100));

        // Below minimum gets clamped to 0.1
        let clamped = img.scale(0.05).unwrap();
        assert_eq!(clamped.dimensions(), (100, 100));
    }

    #[test]
    fn test_scale_max_factor() {
        let img = ImageBuffer::from_test_pattern(100, 100);

        // Maximum scale factor (2.0 = 200%)
        let big = img.scale(2.0).unwrap();
        assert_eq!(big.dimensions(), (200, 200));

        // Above maximum gets clamped to 2.0
        let clamped = img.scale(3.0).unwrap();
        assert_eq!(clamped.dimensions(), (200, 200));
    }

    #[test]
    fn test_scale_preserves_aspect_ratio() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);

        let scaled = img.scale(0.5).unwrap();
        assert_eq!(scaled.dimensions(), (960, 540));

        // Check aspect ratio is preserved
        let (orig_w, orig_h) = img.dimensions();
        let (scaled_w, scaled_h) = scaled.dimensions();
        let orig_ratio = orig_w as f32 / orig_h as f32;
        let scaled_ratio = scaled_w as f32 / scaled_h as f32;
        assert!((orig_ratio - scaled_ratio).abs() < 0.01);
    }

    #[test]
    fn test_crop_valid_region() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);

        // Crop a centered region
        let region = Region::new(460, 240, 1000, 600);
        let cropped = img.crop(region).unwrap();
        assert_eq!(cropped.dimensions(), (1000, 600));
    }

    #[test]
    fn test_crop_boundary_check() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);

        // Crop at exact image edges (should succeed)
        let region = Region::new(0, 0, 1920, 1080);
        let cropped = img.crop(region).unwrap();
        assert_eq!(cropped.dimensions(), (1920, 1080));

        // Crop from corner
        let region = Region::new(0, 0, 100, 100);
        let cropped = img.crop(region).unwrap();
        assert_eq!(cropped.dimensions(), (100, 100));

        // Crop to bottom-right corner
        let region = Region::new(1820, 980, 100, 100);
        let cropped = img.crop(region).unwrap();
        assert_eq!(cropped.dimensions(), (100, 100));
    }

    #[test]
    fn test_crop_out_of_bounds() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);

        // Region starts outside image
        let region = Region::new(2000, 1000, 100, 100);
        assert!(img.crop(region).is_err());

        // Region extends beyond image
        let region = Region::new(1900, 1000, 200, 200);
        assert!(img.crop(region).is_err());

        // Region height extends beyond image
        let region = Region::new(100, 100, 100, 1000);
        assert!(img.crop(region).is_err());
    }

    #[test]
    fn test_to_rgba8_conversion() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let rgba = img.to_rgba8();

        assert_eq!(rgba.dimensions(), (100, 100));

        // RGBA8 should have 4 bytes per pixel
        let (width, height) = rgba.dimensions();
        assert_eq!(rgba.len(), (width * height * 4) as usize);
    }

    #[test]
    fn test_as_bytes_access() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let bytes = img.as_bytes();

        // Should have pixel data
        assert!(!bytes.is_empty());

        // Should be able to access bytes
        let _first_byte = bytes[0];
    }

    #[test]
    fn test_from_test_pattern() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);
        assert_eq!(img.dimensions(), (1920, 1080));

        let img = ImageBuffer::from_test_pattern(100, 100);
        assert_eq!(img.dimensions(), (100, 100));

        // Verify it's not all zeros (has actual pattern)
        let bytes = img.as_bytes();
        let non_zero = bytes.iter().any(|&b| b != 0);
        assert!(non_zero, "Test pattern should contain non-zero pixels");
    }

    #[test]
    fn test_scale_then_crop() {
        let img = ImageBuffer::from_test_pattern(1920, 1080);

        // Scale down then crop
        let scaled = img.scale(0.5).unwrap();
        assert_eq!(scaled.dimensions(), (960, 540));

        let region = Region::new(100, 100, 500, 300);
        let cropped = scaled.crop(region).unwrap();
        assert_eq!(cropped.dimensions(), (500, 300));
    }

    #[test]
    fn test_inner_access() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let inner = img.inner();
        assert_eq!(inner.dimensions(), (100, 100));
    }

    #[test]
    fn test_into_inner() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let dynamic = img.into_inner();
        assert_eq!(dynamic.dimensions(), (100, 100));
    }

    #[test]
    fn test_clone() {
        let img = ImageBuffer::from_test_pattern(100, 100);
        let cloned = img.clone();
        assert_eq!(img.dimensions(), cloned.dimensions());
    }
}
