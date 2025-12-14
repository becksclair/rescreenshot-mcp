// Utility modules

pub mod detect;

#[cfg(feature = "image-processing")]
pub mod encode;

#[cfg(feature = "image-processing")]
pub mod temp_files;

#[cfg(feature = "image-processing")]
pub mod mcp_content;

#[cfg(target_os = "linux")]
pub mod key_store;
