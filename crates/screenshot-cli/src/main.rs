//! screenshot-cli: Command-line tool for screenshot capture debugging
//!
//! Provides commands for listing windows, capturing screenshots, and managing
//! Wayland consent tokens without the MCP protocol overhead.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use screenshot_core::capture::create_default_backend;
use screenshot_core::model::{CaptureOptions, CaptureSource, ImageFormat, WindowSelector};
use screenshot_core::util::encode::encode_image;

#[derive(Parser)]
#[command(name = "screenshot-cli")]
#[command(about = "CLI tool for screenshot capture debugging and testing")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all capturable windows
    ListWindows,
    /// Capture a screenshot of a specific window
    CaptureWindow {
        /// Window title substring or regex pattern
        #[arg(long)]
        title: Option<String>,
        /// Window class name
        #[arg(long)]
        class: Option<String>,
        /// Executable name
        #[arg(long)]
        exe: Option<String>,
        /// Output file path
        #[arg(short, long)]
        out: PathBuf,
        /// Image format (png, jpeg, webp)
        #[arg(long, default_value = "png")]
        format: String,
        /// Image quality (0-100, for JPEG/WebP)
        #[arg(long, default_value_t = 80)]
        quality: u8,
        /// Scale factor (0.1-2.0)
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
    },
    /// Capture a screenshot of an entire display
    CaptureDisplay {
        /// Display ID (0 = primary, 1 = secondary, etc.)
        #[arg(long)]
        display_id: Option<u32>,
        /// Output file path
        #[arg(short, long)]
        out: PathBuf,
        /// Image format (png, jpeg, webp)
        #[arg(long, default_value = "png")]
        format: String,
        /// Image quality (0-100, for JPEG/WebP)
        #[arg(long, default_value_t = 80)]
        quality: u8,
        /// Scale factor (0.1-2.0)
        #[arg(long, default_value_t = 1.0)]
        scale: f32,
    },
    /// Prime Wayland consent for headless capture (Linux Wayland only)
    #[cfg(target_os = "linux")]
    PrimeWaylandConsent {
        /// Source type: monitor, window, or virtual
        #[arg(long, default_value = "monitor")]
        source_type: String,
        /// Stable identifier for this source
        #[arg(long, default_value = "wayland-default")]
        source_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("screenshot_cli=info".parse()?)
                .add_directive("screenshot_core=warn".parse()?),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::ListWindows => {
            list_windows().await?;
        }
        Commands::CaptureWindow {
            title,
            class,
            exe,
            out,
            format,
            quality,
            scale,
        } => {
            capture_window(title, class, exe, out, format, quality, scale).await?;
        }
        Commands::CaptureDisplay {
            display_id,
            out,
            format,
            quality,
            scale,
        } => {
            capture_display(display_id, out, format, quality, scale).await?;
        }
        #[cfg(target_os = "linux")]
        Commands::PrimeWaylandConsent {
            source_type,
            source_id,
        } => {
            prime_wayland_consent(source_type, source_id).await?;
        }
    }

    Ok(())
}

async fn list_windows() -> Result<()> {
    let backend = create_default_backend()?;
    let windows = backend.list_windows().await?;

    println!("Found {} windows:\n", windows.len());
    for window in windows {
        println!("  ID: {}", window.id);
        println!("  Title: {}", window.title);
        if !window.class.is_empty() {
            println!("  Class: {}", window.class);
        }
        if !window.owner.is_empty() {
            println!("  Executable: {}", window.owner);
        }
        if window.pid > 0 {
            println!("  PID: {}", window.pid);
        }
        println!();
    }

    Ok(())
}

async fn capture_window(
    title: Option<String>,
    class: Option<String>,
    exe: Option<String>,
    out: PathBuf,
    format_str: String,
    quality: u8,
    scale: f32,
) -> Result<()> {
    // Validate selector
    if title.is_none() && class.is_none() && exe.is_none() {
        anyhow::bail!("At least one of --title, --class, or --exe must be specified");
    }

    // Parse format
    let format = match format_str.to_lowercase().as_str() {
        "png" => ImageFormat::Png,
        "jpeg" | "jpg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::Webp,
        _ => anyhow::bail!("Invalid format '{}'. Must be png, jpeg, or webp", format_str),
    };

    // Validate quality
    if quality > 100 {
        anyhow::bail!("Quality must be between 0 and 100");
    }

    // Validate scale
    if !(0.1..=2.0).contains(&scale) {
        anyhow::bail!("Scale must be between 0.1 and 2.0");
    }

    // Create backend
    let backend = create_default_backend()?;

    // Build selector
    let selector = WindowSelector {
        title_substring_or_regex: title,
        class,
        exe,
    };

    // Resolve window
    println!("Resolving window...");
    let handle = backend.resolve_target(&selector).await?;
    println!("Found window: {}", handle);

    // Capture
    println!("Capturing window...");
    let opts = CaptureOptions::builder()
        .format(format)
        .quality(quality)
        .scale(scale)
        .build();

    let source = CaptureSource::Window(handle);
    let image_buffer = backend.capture(source, &opts).await?;

    // Encode and save
    println!("Encoding image...");
    let image_data = encode_image(&image_buffer, &opts)?;

    println!("Saving to {}...", out.display());
    fs::write(&out, image_data)?;

    println!("✓ Screenshot saved to {}", out.display());
    Ok(())
}

async fn capture_display(
    display_id: Option<u32>,
    out: PathBuf,
    format_str: String,
    quality: u8,
    scale: f32,
) -> Result<()> {
    // Parse format
    let format = match format_str.to_lowercase().as_str() {
        "png" => ImageFormat::Png,
        "jpeg" | "jpg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::Webp,
        _ => anyhow::bail!("Invalid format '{}'. Must be png, jpeg, or webp", format_str),
    };

    // Validate quality
    if quality > 100 {
        anyhow::bail!("Quality must be between 0 and 100");
    }

    // Validate scale
    if !(0.1..=2.0).contains(&scale) {
        anyhow::bail!("Scale must be between 0.1 and 2.0");
    }

    // Create backend
    let backend = create_default_backend()?;

    // Capture
    let display_name = display_id
        .map(|id| format!("display {}", id))
        .unwrap_or_else(|| "primary display".to_string());
    println!("Capturing {}...", display_name);

    let opts = CaptureOptions::builder()
        .format(format)
        .quality(quality)
        .scale(scale)
        .build();

    let source = CaptureSource::Display(display_id);
    let image_buffer = backend.capture(source, &opts).await?;

    // Encode and save
    println!("Encoding image...");
    let image_data = encode_image(&image_buffer, &opts)?;

    println!("Saving to {}...", out.display());
    fs::write(&out, image_data)?;

    println!("✓ Screenshot saved to {}", out.display());
    Ok(())
}

#[cfg(target_os = "linux")]
async fn prime_wayland_consent(source_type_str: String, source_id: String) -> Result<()> {
    use screenshot_core::capture::wayland_backend::WaylandBackend;
    use screenshot_core::model::SourceType;
    use screenshot_core::util::key_store::KeyStore;

    // Parse source type
    let source_type = match source_type_str.to_lowercase().as_str() {
        "monitor" => SourceType::Monitor,
        "window" => SourceType::Window,
        "virtual" => SourceType::Virtual,
        _ => anyhow::bail!(
            "Invalid source_type '{}'. Must be monitor, window, or virtual",
            source_type_str
        ),
    };

    // Create backend
    use std::sync::Arc;
    let key_store = Arc::new(KeyStore::new());
    let backend = WaylandBackend::new(key_store);

    println!("Priming Wayland consent...");
    println!("  Source type: {:?}", source_type);
    println!("  Source ID: {}", source_id);
    println!();
    println!("Please grant permission in the portal dialog...");

    let result = backend
        .prime_consent(source_type, &source_id, false)
        .await?;

    println!();
    println!("✓ Consent primed successfully");
    println!("  Primary source ID: {}", result.primary_source_id);
    println!("  Number of streams: {}", result.num_streams);
    println!();
    println!("You can now use headless capture with source_id: {}", source_id);

    Ok(())
}
