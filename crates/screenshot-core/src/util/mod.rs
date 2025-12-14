// Utility modules

pub mod detect;

#[cfg(feature = "image-processing")]
pub mod encode;

#[cfg(feature = "image-processing")]
pub mod temp_files;

#[cfg(target_os = "linux")]
pub mod key_store;
