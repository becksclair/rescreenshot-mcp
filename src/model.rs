//! Data models and type definitions for screenshot-mcp
//!
//! This module defines the core types used throughout the application:
//! - Platform and backend detection types
//! - Health check response structures
//! - Serialization/deserialization support for MCP protocol

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Represents the display backend type for the current platform
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    /// No backend detected or not yet initialized
    None,
    /// Wayland display server (Linux)
    Wayland,
    /// X11 display server (Linux)
    X11,
    /// Windows Graphics Capture API
    Windows,
    /// macOS ScreenCaptureKit
    #[serde(rename = "macos")]
    MacOS,
}

impl BackendType {
    /// Returns the backend type as a lowercase string
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendType::None => "none",
            BackendType::Wayland => "wayland",
            BackendType::X11 => "x11",
            BackendType::Windows => "windows",
            BackendType::MacOS => "macos",
        }
    }
}

impl std::fmt::Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Image format for encoded screenshots
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    /// PNG format (lossless, larger file size)
    #[default]
    Png,
    /// WebP format (lossy/lossless, good compression)
    Webp,
    /// JPEG format (lossy, smallest file size)
    Jpeg,
}

impl ImageFormat {
    /// Returns the file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Webp => "webp",
            ImageFormat::Jpeg => "jpg",
        }
    }

    /// Returns the MIME type for this format
    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Webp => "image/webp",
            ImageFormat::Jpeg => "image/jpeg",
        }
    }
}

impl std::fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageFormat::Png => write!(f, "png"),
            ImageFormat::Webp => write!(f, "webp"),
            ImageFormat::Jpeg => write!(f, "jpeg"),
        }
    }
}

/// Wayland-specific capture source (placeholder for M2 implementation)
///
/// This enum will be expanded in M2 to support Wayland portal restore tokens
/// and different source types (screen, window, selection).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WaylandSource {
    /// Placeholder variant - to be implemented in M2
    NotYetImplemented,
}

/// Type alias for window handle identifiers
///
/// Window handles are platform-specific strings used to identify windows.
/// The exact format depends on the backend (HWND on Windows, window ID on X11,
/// etc.)
pub type WindowHandle = String;

/// Rectangular region for partial screen capture
///
/// Coordinates are in pixels, with (0, 0) at the top-left corner of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Region {
    /// X coordinate of the top-left corner
    pub x:      u32,
    /// Y coordinate of the top-left corner
    pub y:      u32,
    /// Width of the region in pixels
    pub width:  u32,
    /// Height of the region in pixels
    pub height: u32,
}

impl Region {
    /// Creates a new Region
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Selector for identifying target windows
///
/// Used to locate windows by title, class, or executable name.
/// At least one field must be specified. If multiple fields are provided,
/// they are combined with AND logic.
///
/// # Examples
///
/// ```
/// use screenshot_mcp::model::WindowSelector;
///
/// // Select by title substring
/// let selector = WindowSelector {
///     title_substring_or_regex: Some("Firefox".to_string()),
///     class: None,
///     exe: None,
/// };
///
/// // Select by class and exe
/// let selector = WindowSelector {
///     title_substring_or_regex: None,
///     class: Some("Alacritty".to_string()),
///     exe: Some("alacritty".to_string()),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WindowSelector {
    /// Window title substring or regex pattern
    pub title_substring_or_regex: Option<String>,
    /// Window class name
    pub class: Option<String>,
    /// Executable name
    pub exe: Option<String>,
}

impl WindowSelector {
    /// Creates a WindowSelector that matches by title
    pub fn by_title(title: impl Into<String>) -> Self {
        Self {
            title_substring_or_regex: Some(title.into()),
            class: None,
            exe: None,
        }
    }

    /// Creates a WindowSelector that matches by class
    pub fn by_class(class: impl Into<String>) -> Self {
        Self {
            title_substring_or_regex: None,
            class: Some(class.into()),
            exe: None,
        }
    }

    /// Creates a WindowSelector that matches by executable
    pub fn by_exe(exe: impl Into<String>) -> Self {
        Self {
            title_substring_or_regex: None,
            class: None,
            exe: Some(exe.into()),
        }
    }
}

/// Information about a window
///
/// Contains metadata about a window that can be captured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WindowInfo {
    /// Platform-specific window identifier
    pub id:      WindowHandle,
    /// Window title
    pub title:   String,
    /// Window class name
    pub class:   String,
    /// Window owner/application name
    pub owner:   String,
    /// Process ID of the window owner
    pub pid:     u32,
    /// Backend that detected this window
    pub backend: BackendType,
}

impl WindowInfo {
    /// Creates a new WindowInfo
    pub fn new(
        id: WindowHandle,
        title: String,
        class: String,
        owner: String,
        pid: u32,
        backend: BackendType,
    ) -> Self {
        Self {
            id,
            title,
            class,
            owner,
            pid,
            backend,
        }
    }
}

/// Backend capabilities for screenshot capture
///
/// Different backends support different features. This struct describes
/// what a particular backend can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    /// Backend supports capturing cursor in screenshots
    pub supports_cursor:          bool,
    /// Backend supports partial region capture
    pub supports_region:          bool,
    /// Backend supports Wayland restore tokens (for permission-free recapture)
    pub supports_wayland_restore: bool,
    /// Backend supports window-specific capture
    pub supports_window_capture:  bool,
    /// Backend supports full display/screen capture
    pub supports_display_capture: bool,
}

impl Capabilities {
    /// Creates a Capabilities struct with all features enabled
    pub fn full() -> Self {
        Self {
            supports_cursor:          true,
            supports_region:          true,
            supports_wayland_restore: true,
            supports_window_capture:  true,
            supports_display_capture: true,
        }
    }

    /// Creates a Capabilities struct with all features disabled
    pub fn none() -> Self {
        Self {
            supports_cursor:          false,
            supports_region:          false,
            supports_wayland_restore: false,
            supports_window_capture:  false,
            supports_display_capture: false,
        }
    }
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::none()
    }
}

/// Options for screenshot capture
///
/// Controls the output format, quality, scaling, and capture behavior.
///
/// # Examples
///
/// ```
/// use screenshot_mcp::model::{CaptureOptions, ImageFormat};
///
/// // Default options (PNG, quality=80, scale=1.0)
/// let opts = CaptureOptions::default();
///
/// // Custom WebP with high quality
/// let opts = CaptureOptions {
///     format:         ImageFormat::Webp,
///     quality:        90,
///     scale:          1.0,
///     include_cursor: false,
///     region:         None,
///     wayland_source: None,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CaptureOptions {
    /// Output image format
    #[serde(default)]
    pub format:         ImageFormat,
    /// JPEG/WebP quality (0-100, clamped if out of range)
    #[serde(default = "default_quality")]
    pub quality:        u8,
    /// Scale factor for output image (0.1-2.0, clamped if out of range)
    #[serde(default = "default_scale")]
    pub scale:          f32,
    /// Whether to include cursor in screenshot
    #[serde(default)]
    pub include_cursor: bool,
    /// Optional region to capture (None = full screen/window)
    #[serde(default)]
    pub region:         Option<Region>,
    /// Wayland-specific source (for restore tokens, M2)
    #[serde(default)]
    pub wayland_source: Option<WaylandSource>,
}

fn default_quality() -> u8 {
    80
}

fn default_scale() -> f32 {
    1.0
}

impl CaptureOptions {
    /// Creates CaptureOptions with default values and validates parameters
    pub fn new() -> Self {
        Self::default()
    }

    /// Validates and clamps quality and scale parameters
    pub fn validate(&mut self) {
        self.quality = self.quality.clamp(0, 100);
        self.scale = self.scale.clamp(0.1, 2.0);
    }

    /// Creates a builder for CaptureOptions
    pub fn builder() -> CaptureOptionsBuilder {
        CaptureOptionsBuilder::default()
    }
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            format:         ImageFormat::default(),
            quality:        default_quality(),
            scale:          default_scale(),
            include_cursor: false,
            region:         None,
            wayland_source: None,
        }
    }
}

/// Builder for CaptureOptions
#[derive(Debug, Clone, Default)]
pub struct CaptureOptionsBuilder {
    options: CaptureOptions,
}

impl CaptureOptionsBuilder {
    /// Sets the image format
    pub fn format(mut self, format: ImageFormat) -> Self {
        self.options.format = format;
        self
    }

    /// Sets the quality (0-100)
    pub fn quality(mut self, quality: u8) -> Self {
        self.options.quality = quality;
        self
    }

    /// Sets the scale factor (0.1-2.0)
    pub fn scale(mut self, scale: f32) -> Self {
        self.options.scale = scale;
        self
    }

    /// Sets whether to include cursor
    pub fn include_cursor(mut self, include: bool) -> Self {
        self.options.include_cursor = include;
        self
    }

    /// Sets the capture region
    pub fn region(mut self, region: Region) -> Self {
        self.options.region = Some(region);
        self
    }

    /// Sets the Wayland source
    pub fn wayland_source(mut self, source: WaylandSource) -> Self {
        self.options.wayland_source = Some(source);
        self
    }

    /// Builds CaptureOptions and validates parameters
    pub fn build(mut self) -> CaptureOptions {
        self.options.validate();
        self.options
    }
}

/// Platform information including OS and display backend
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlatformInfo {
    /// Operating system name (e.g., "linux", "windows", "macos")
    pub os:      String,
    /// Detected display backend
    pub backend: BackendType,
}

impl PlatformInfo {
    /// Creates a new PlatformInfo instance
    pub fn new(os: String, backend: BackendType) -> Self {
        Self { os, backend }
    }
}

/// Response structure for the health_check MCP tool
///
/// This is returned by the `health_check` tool to indicate server status
/// and platform detection results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HealthCheckResponse {
    /// Platform/OS name
    pub platform: String,
    /// Backend type as a string
    pub backend:  String,
    /// Whether the server is functioning correctly
    pub ok:       bool,
}

impl HealthCheckResponse {
    /// Creates a successful health check response from platform info
    pub fn from_platform(info: PlatformInfo) -> Self {
        Self {
            platform: info.os,
            backend:  info.backend.as_str().to_string(),
            ok:       true,
        }
    }

    /// Creates a health check response indicating an error state
    pub fn error(platform: String, backend: String) -> Self {
        Self {
            platform,
            backend,
            ok: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_type_serialization() {
        // Test that BackendType serializes to lowercase strings
        assert_eq!(serde_json::to_string(&BackendType::None).unwrap(), r#""none""#);
        assert_eq!(serde_json::to_string(&BackendType::Wayland).unwrap(), r#""wayland""#);
        assert_eq!(serde_json::to_string(&BackendType::X11).unwrap(), r#""x11""#);
        assert_eq!(serde_json::to_string(&BackendType::Windows).unwrap(), r#""windows""#);
        assert_eq!(serde_json::to_string(&BackendType::MacOS).unwrap(), r#""macos""#);
    }

    #[test]
    fn test_backend_type_deserialization() {
        // Test that lowercase strings deserialize to BackendType
        assert_eq!(serde_json::from_str::<BackendType>(r#""none""#).unwrap(), BackendType::None);
        assert_eq!(
            serde_json::from_str::<BackendType>(r#""wayland""#).unwrap(),
            BackendType::Wayland
        );
        assert_eq!(serde_json::from_str::<BackendType>(r#""x11""#).unwrap(), BackendType::X11);
        assert_eq!(
            serde_json::from_str::<BackendType>(r#""windows""#).unwrap(),
            BackendType::Windows
        );
        assert_eq!(serde_json::from_str::<BackendType>(r#""macos""#).unwrap(), BackendType::MacOS);
    }

    #[test]
    fn test_backend_type_as_str() {
        assert_eq!(BackendType::None.as_str(), "none");
        assert_eq!(BackendType::Wayland.as_str(), "wayland");
        assert_eq!(BackendType::X11.as_str(), "x11");
        assert_eq!(BackendType::Windows.as_str(), "windows");
        assert_eq!(BackendType::MacOS.as_str(), "macos");
    }

    #[test]
    fn test_backend_type_display() {
        assert_eq!(format!("{}", BackendType::None), "none");
        assert_eq!(format!("{}", BackendType::Wayland), "wayland");
        assert_eq!(format!("{}", BackendType::X11), "x11");
    }

    #[test]
    fn test_platform_info_serialization() {
        let info = PlatformInfo::new("linux".to_string(), BackendType::Wayland);
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains(r#""os":"linux""#));
        assert!(json.contains(r#""backend":"wayland""#));
    }

    #[test]
    fn test_platform_info_deserialization() {
        let json = r#"{"os":"linux","backend":"wayland"}"#;
        let info: PlatformInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.os, "linux");
        assert_eq!(info.backend, BackendType::Wayland);
    }

    #[test]
    fn test_health_check_response_from_platform() {
        let info = PlatformInfo::new("linux".to_string(), BackendType::X11);
        let response = HealthCheckResponse::from_platform(info);

        assert_eq!(response.platform, "linux");
        assert_eq!(response.backend, "x11");
        assert!(response.ok);
    }

    #[test]
    fn test_health_check_response_serialization() {
        let response = HealthCheckResponse {
            platform: "linux".to_string(),
            backend:  "wayland".to_string(),
            ok:       true,
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["platform"], "linux");
        assert_eq!(json["backend"], "wayland");
        assert_eq!(json["ok"], true);
    }

    #[test]
    fn test_health_check_response_deserialization() {
        let json = r#"{"platform":"windows","backend":"windows","ok":true}"#;
        let response: HealthCheckResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.platform, "windows");
        assert_eq!(response.backend, "windows");
        assert!(response.ok);
    }

    #[test]
    fn test_health_check_response_error_state() {
        let response = HealthCheckResponse::error("linux".to_string(), "none".to_string());

        assert_eq!(response.platform, "linux");
        assert_eq!(response.backend, "none");
        assert!(!response.ok);
    }

    #[test]
    fn test_json_schema_generation() {
        // Verify that types implement JsonSchema
        let _backend_schema = schemars::schema_for!(BackendType);
        let _platform_schema = schemars::schema_for!(PlatformInfo);
        let _health_schema = schemars::schema_for!(HealthCheckResponse);
    }

    // ========== M1 Phase 1 Tests ==========

    #[test]
    fn test_image_format_serialization() {
        assert_eq!(serde_json::to_string(&ImageFormat::Png).unwrap(), r#""png""#);
        assert_eq!(serde_json::to_string(&ImageFormat::Webp).unwrap(), r#""webp""#);
        assert_eq!(serde_json::to_string(&ImageFormat::Jpeg).unwrap(), r#""jpeg""#);
    }

    #[test]
    fn test_image_format_deserialization() {
        assert_eq!(serde_json::from_str::<ImageFormat>(r#""png""#).unwrap(), ImageFormat::Png);
        assert_eq!(serde_json::from_str::<ImageFormat>(r#""webp""#).unwrap(), ImageFormat::Webp);
        assert_eq!(serde_json::from_str::<ImageFormat>(r#""jpeg""#).unwrap(), ImageFormat::Jpeg);
    }

    #[test]
    fn test_image_format_extension() {
        assert_eq!(ImageFormat::Png.extension(), "png");
        assert_eq!(ImageFormat::Webp.extension(), "webp");
        assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
    }

    #[test]
    fn test_image_format_mime_type() {
        assert_eq!(ImageFormat::Png.mime_type(), "image/png");
        assert_eq!(ImageFormat::Webp.mime_type(), "image/webp");
        assert_eq!(ImageFormat::Jpeg.mime_type(), "image/jpeg");
    }

    #[test]
    fn test_image_format_display() {
        assert_eq!(format!("{}", ImageFormat::Png), "png");
        assert_eq!(format!("{}", ImageFormat::Webp), "webp");
        assert_eq!(format!("{}", ImageFormat::Jpeg), "jpeg");
    }

    #[test]
    fn test_image_format_default() {
        assert_eq!(ImageFormat::default(), ImageFormat::Png);
    }

    #[test]
    fn test_wayland_source_serialization() {
        let source = WaylandSource::NotYetImplemented;
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(json, r#""not_yet_implemented""#);
    }

    #[test]
    fn test_region_creation() {
        let region = Region::new(10, 20, 800, 600);
        assert_eq!(region.x, 10);
        assert_eq!(region.y, 20);
        assert_eq!(region.width, 800);
        assert_eq!(region.height, 600);
    }

    #[test]
    fn test_region_serialization() {
        let region = Region::new(0, 0, 1920, 1080);
        let json = serde_json::to_value(region).unwrap();
        assert_eq!(json["x"], 0);
        assert_eq!(json["y"], 0);
        assert_eq!(json["width"], 1920);
        assert_eq!(json["height"], 1080);
    }

    #[test]
    fn test_region_deserialization() {
        let json = r#"{"x":100,"y":200,"width":640,"height":480}"#;
        let region: Region = serde_json::from_str(json).unwrap();
        assert_eq!(region.x, 100);
        assert_eq!(region.y, 200);
        assert_eq!(region.width, 640);
        assert_eq!(region.height, 480);
    }

    #[test]
    fn test_window_selector_by_title() {
        let selector = WindowSelector::by_title("Firefox");
        assert_eq!(selector.title_substring_or_regex, Some("Firefox".to_string()));
        assert_eq!(selector.class, None);
        assert_eq!(selector.exe, None);
    }

    #[test]
    fn test_window_selector_by_class() {
        let selector = WindowSelector::by_class("Alacritty");
        assert_eq!(selector.title_substring_or_regex, None);
        assert_eq!(selector.class, Some("Alacritty".to_string()));
        assert_eq!(selector.exe, None);
    }

    #[test]
    fn test_window_selector_by_exe() {
        let selector = WindowSelector::by_exe("code");
        assert_eq!(selector.title_substring_or_regex, None);
        assert_eq!(selector.class, None);
        assert_eq!(selector.exe, Some("code".to_string()));
    }

    #[test]
    fn test_window_selector_serialization() {
        let selector = WindowSelector::by_title("VSCode");
        let json = serde_json::to_value(&selector).unwrap();
        assert_eq!(json["title_substring_or_regex"], "VSCode");
        assert_eq!(json["class"], serde_json::Value::Null);
        assert_eq!(json["exe"], serde_json::Value::Null);
    }

    #[test]
    fn test_window_info_creation() {
        let info = WindowInfo::new(
            "0x123".to_string(),
            "Firefox".to_string(),
            "Navigator".to_string(),
            "firefox".to_string(),
            1234,
            BackendType::X11,
        );
        assert_eq!(info.id, "0x123");
        assert_eq!(info.title, "Firefox");
        assert_eq!(info.class, "Navigator");
        assert_eq!(info.owner, "firefox");
        assert_eq!(info.pid, 1234);
        assert_eq!(info.backend, BackendType::X11);
    }

    #[test]
    fn test_window_info_serialization() {
        let info = WindowInfo::new(
            "hwnd_456".to_string(),
            "VSCode".to_string(),
            "Code".to_string(),
            "code.exe".to_string(),
            5678,
            BackendType::Windows,
        );
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["id"], "hwnd_456");
        assert_eq!(json["title"], "VSCode");
        assert_eq!(json["backend"], "windows");
        assert_eq!(json["pid"], 5678);
    }

    #[test]
    fn test_capabilities_full() {
        let caps = Capabilities::full();
        assert!(caps.supports_cursor);
        assert!(caps.supports_region);
        assert!(caps.supports_wayland_restore);
        assert!(caps.supports_window_capture);
        assert!(caps.supports_display_capture);
    }

    #[test]
    fn test_capabilities_none() {
        let caps = Capabilities::none();
        assert!(!caps.supports_cursor);
        assert!(!caps.supports_region);
        assert!(!caps.supports_wayland_restore);
        assert!(!caps.supports_window_capture);
        assert!(!caps.supports_display_capture);
    }

    #[test]
    fn test_capabilities_default() {
        let caps = Capabilities::default();
        assert_eq!(caps, Capabilities::none());
    }

    #[test]
    fn test_capabilities_serialization() {
        let caps = Capabilities::full();
        let json = serde_json::to_value(caps).unwrap();
        assert_eq!(json["supports_cursor"], true);
        assert_eq!(json["supports_region"], true);
        assert_eq!(json["supports_wayland_restore"], true);
        assert_eq!(json["supports_window_capture"], true);
        assert_eq!(json["supports_display_capture"], true);
    }

    #[test]
    fn test_capture_options_default() {
        let opts = CaptureOptions::default();
        assert_eq!(opts.format, ImageFormat::Png);
        assert_eq!(opts.quality, 80);
        assert_eq!(opts.scale, 1.0);
        assert!(!opts.include_cursor);
        assert_eq!(opts.region, None);
        assert_eq!(opts.wayland_source, None);
    }

    #[test]
    fn test_capture_options_quality_validation() {
        let mut opts = CaptureOptions {
            format:         ImageFormat::Webp,
            quality:        150,
            scale:          1.0,
            include_cursor: false,
            region:         None,
            wayland_source: None,
        };
        opts.validate();
        assert_eq!(opts.quality, 100);

        opts.quality = 255;
        opts.validate();
        assert_eq!(opts.quality, 100);
    }

    #[test]
    fn test_capture_options_scale_validation() {
        let mut opts = CaptureOptions {
            format:         ImageFormat::Png,
            quality:        80,
            scale:          3.0,
            include_cursor: false,
            region:         None,
            wayland_source: None,
        };
        opts.validate();
        assert_eq!(opts.scale, 2.0);

        opts.scale = 0.05;
        opts.validate();
        assert_eq!(opts.scale, 0.1);
    }

    #[test]
    fn test_capture_options_builder() {
        let opts = CaptureOptions::builder()
            .format(ImageFormat::Webp)
            .quality(90)
            .scale(0.5)
            .include_cursor(true)
            .region(Region::new(0, 0, 800, 600))
            .build();

        assert_eq!(opts.format, ImageFormat::Webp);
        assert_eq!(opts.quality, 90);
        assert_eq!(opts.scale, 0.5);
        assert!(opts.include_cursor);
        assert_eq!(opts.region, Some(Region::new(0, 0, 800, 600)));
    }

    #[test]
    fn test_capture_options_builder_validation() {
        let opts = CaptureOptions::builder().quality(150).scale(5.0).build();

        assert_eq!(opts.quality, 100);
        assert_eq!(opts.scale, 2.0);
    }

    #[test]
    fn test_capture_options_serialization() {
        let opts = CaptureOptions {
            format:         ImageFormat::Jpeg,
            quality:        85,
            scale:          1.5,
            include_cursor: true,
            region:         Some(Region::new(10, 20, 640, 480)),
            wayland_source: None,
        };

        let json = serde_json::to_value(&opts).unwrap();
        assert_eq!(json["format"], "jpeg");
        assert_eq!(json["quality"], 85);
        assert_eq!(json["scale"], 1.5);
        assert_eq!(json["include_cursor"], true);
        assert!(json["region"].is_object());
    }

    #[test]
    fn test_capture_options_deserialization_with_defaults() {
        let json = r#"{}"#;
        let opts: CaptureOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.format, ImageFormat::Png);
        assert_eq!(opts.quality, 80);
        assert_eq!(opts.scale, 1.0);
        assert!(!opts.include_cursor);
    }

    #[test]
    fn test_new_types_json_schema_generation() {
        let _image_format_schema = schemars::schema_for!(ImageFormat);
        let _wayland_source_schema = schemars::schema_for!(WaylandSource);
        let _region_schema = schemars::schema_for!(Region);
        let _window_selector_schema = schemars::schema_for!(WindowSelector);
        let _window_info_schema = schemars::schema_for!(WindowInfo);
        let _capabilities_schema = schemars::schema_for!(Capabilities);
        let _capture_options_schema = schemars::schema_for!(CaptureOptions);
    }
}
