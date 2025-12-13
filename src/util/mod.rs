// Utility modules

pub mod detect;

#[cfg(feature = "image-processing")]
pub mod encode;

#[cfg(feature = "image-processing")]
pub mod temp_files;

#[cfg(feature = "image-processing")]
pub mod mcp_content;

#[cfg(all(target_os = "linux", feature = "linux-wayland"))]
pub mod key_store;
