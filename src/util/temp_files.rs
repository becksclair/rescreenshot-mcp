//! Temporary file management for screenshot capture
//!
//! This module provides a thread-safe temporary file manager that tracks
//! all created screenshot files and ensures they are cleaned up when the
//! manager is dropped. Files are stored in a dedicated subdirectory within
//! the system's temporary directory.
//!
//! # Examples
//!
//! ```
//! use screenshot_mcp::{model::ImageFormat, util::temp_files::TempFileManager};
//!
//! let manager = TempFileManager::new();
//!
//! // Create a temp file with custom prefix and extension
//! let path = manager.create_temp_file("screenshot", "png").unwrap();
//! println!("Created temp file: {:?}", path);
//!
//! // Write image data
//! let image_data = vec![0u8; 1024];
//! let (path, size) = manager.write_image(&image_data, ImageFormat::Png).unwrap();
//! println!("Wrote {} bytes to {:?}", size, path);
//!
//! // Files are automatically cleaned up when manager is dropped
//! ```

use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};

use crate::{
    error::{CaptureError, CaptureResult},
    model::ImageFormat,
};

/// Represents a tracked temporary file
///
/// Stores metadata about a temporary file created by the manager,
/// including its path and creation timestamp.
#[derive(Debug, Clone)]
pub struct TempFile {
    /// Path to the temporary file
    pub path:      PathBuf,
    /// Timestamp when the file was created
    pub timestamp: DateTime<Utc>,
}

impl TempFile {
    /// Creates a new TempFile record
    pub fn new(path: PathBuf, timestamp: DateTime<Utc>) -> Self {
        Self { path, timestamp }
    }
}

/// Thread-safe temporary file manager
///
/// Manages the lifecycle of temporary screenshot files, ensuring they are
/// tracked and cleaned up properly. Files are stored in
/// `$TEMP_DIR/screenshot-mcp/` with unique timestamped filenames.
///
/// The manager uses `Arc<Mutex<Vec<TempFile>>>` internally for thread-safe
/// access, allowing it to be safely cloned and shared across threads.
///
/// # Cleanup
///
/// All tracked files are automatically deleted when the manager is dropped.
/// Cleanup is best-effort: errors are logged but don't cause panics.
///
/// # Examples
///
/// ```
/// use screenshot_mcp::{model::ImageFormat, util::temp_files::TempFileManager};
///
/// let manager = TempFileManager::new();
///
/// // Create multiple temp files
/// let file1 = manager.create_temp_file("screenshot", "png").unwrap();
/// let file2 = manager.create_temp_file("screenshot", "jpg").unwrap();
///
/// // Files are cleaned up automatically when manager goes out of scope
/// ```
#[derive(Clone, Debug)]
pub struct TempFileManager {
    /// Internal storage for tracked temp files
    files: Arc<Mutex<Vec<TempFile>>>,
}

impl TempFileManager {
    /// Creates a new empty TempFileManager
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::util::temp_files::TempFileManager;
    ///
    /// let manager = TempFileManager::new();
    /// ```
    pub fn new() -> Self {
        Self {
            files: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Gets the base directory for temporary files
    ///
    /// Returns `$TEMP_DIR/screenshot-mcp/` where `$TEMP_DIR` is the
    /// system's temporary directory.
    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join("screenshot-mcp")
    }

    /// Ensures the temp directory exists, creating it if necessary
    fn ensure_temp_dir() -> CaptureResult<PathBuf> {
        let dir = Self::temp_dir();
        if !dir.exists() {
            fs::create_dir_all(&dir).map_err(CaptureError::IoError)?;
        }
        Ok(dir)
    }

    /// Creates a temporary file with a unique timestamped name
    ///
    /// The filename format is `{prefix}-{timestamp}.{ext}` where timestamp
    /// is an ISO 8601 formatted string with special characters replaced for
    /// filesystem compatibility.
    ///
    /// The file is created (as an empty file) and tracked by the manager.
    /// Parent directories are created if they don't exist.
    ///
    /// # Arguments
    ///
    /// * `prefix` - Prefix for the filename (e.g., "screenshot")
    /// * `ext` - File extension without the dot (e.g., "png", "jpg")
    ///
    /// # Returns
    ///
    /// The full path to the created temporary file
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::util::temp_files::TempFileManager;
    ///
    /// let manager = TempFileManager::new();
    /// let path = manager.create_temp_file("screenshot", "png").unwrap();
    /// assert!(path.exists());
    /// ```
    pub fn create_temp_file(&self, prefix: &str, ext: &str) -> CaptureResult<PathBuf> {
        let dir = Self::ensure_temp_dir()?;

        // Generate unique timestamp-based filename
        let timestamp = Utc::now();
        let timestamp_str: String = timestamp
            .to_rfc3339()
            .chars()
            .map(|c| match c {
                ':' => '-',
                '+' | '.' => '_',
                _ => c,
            })
            .collect();

        let filename = format!("{}-{}.{}", prefix, timestamp_str, ext);
        let path = dir.join(filename);

        // Create the file (empty initially)
        fs::File::create(&path).map_err(CaptureError::IoError)?;

        // Track the file
        let temp_file = TempFile::new(path.clone(), timestamp);
        if let Ok(mut files) = self.files.lock() {
            files.push(temp_file);
        }

        Ok(path)
    }

    /// Writes image data to a temporary file
    ///
    /// Creates a new temp file with a name based on the image format and
    /// writes the provided data to it. The file is tracked for cleanup.
    ///
    /// # Arguments
    ///
    /// * `data` - Encoded image data as bytes
    /// * `format` - Image format (determines file extension)
    ///
    /// # Returns
    ///
    /// A tuple of `(path, size)` where:
    /// - `path` is the full path to the created file
    /// - `size` is the number of bytes written
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::{model::ImageFormat, util::temp_files::TempFileManager};
    ///
    /// let manager = TempFileManager::new();
    /// let image_data = vec![0u8; 1024]; // Dummy image data
    ///
    /// let (path, size) = manager.write_image(&image_data, ImageFormat::Png).unwrap();
    /// assert_eq!(size, 1024);
    /// assert!(path.exists());
    /// ```
    pub fn write_image(&self, data: &[u8], format: ImageFormat) -> CaptureResult<(PathBuf, u64)> {
        let ext = format.extension();
        let path = self.create_temp_file("screenshot", ext)?;

        // Write the data
        fs::write(&path, data).map_err(CaptureError::IoError)?;

        let size = data.len() as u64;
        Ok((path, size))
    }

    /// Manually cleans up all tracked temporary files
    ///
    /// Removes all tracked files from the filesystem and clears the internal
    /// tracking list. This is automatically called when the manager is dropped,
    /// but can be called manually if needed.
    ///
    /// Errors during cleanup are logged but don't cause the method to fail.
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::util::temp_files::TempFileManager;
    ///
    /// let manager = TempFileManager::new();
    /// let path = manager.create_temp_file("screenshot", "png").unwrap();
    /// assert!(path.exists());
    ///
    /// // Manually cleanup
    /// manager.cleanup_all();
    /// assert!(!path.exists());
    /// ```
    pub fn cleanup_all(&self) {
        if let Ok(mut files) = self.files.lock() {
            for temp_file in files.iter() {
                if temp_file.path.exists() {
                    if let Err(e) = fs::remove_file(&temp_file.path) {
                        tracing::warn!("Failed to remove temp file {:?}: {}", temp_file.path, e);
                    }
                }
            }
            files.clear();
        }
    }

    /// Returns the number of tracked temporary files
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::util::temp_files::TempFileManager;
    ///
    /// let manager = TempFileManager::new();
    /// assert_eq!(manager.count(), 0);
    ///
    /// manager.create_temp_file("screenshot", "png").unwrap();
    /// assert_eq!(manager.count(), 1);
    /// ```
    pub fn count(&self) -> usize {
        self.files.lock().map(|files| files.len()).unwrap_or(0)
    }

    /// Returns a list of all tracked file paths
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::util::temp_files::TempFileManager;
    ///
    /// let manager = TempFileManager::new();
    /// manager.create_temp_file("screenshot", "png").unwrap();
    /// manager.create_temp_file("screenshot", "jpg").unwrap();
    ///
    /// let paths = manager.list_files();
    /// assert_eq!(paths.len(), 2);
    /// ```
    pub fn list_files(&self) -> Vec<PathBuf> {
        self.files
            .lock()
            .map(|files| files.iter().map(|f| f.path.clone()).collect())
            .unwrap_or_default()
    }
}

impl Default for TempFileManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TempFileManager {
    /// Automatically cleans up all tracked files when the manager is dropped
    ///
    /// This ensures no temporary files are left behind when the program exits.
    /// Cleanup is best-effort and doesn't panic on errors.
    fn drop(&mut self) {
        // Only cleanup if this is the last reference to the Arc
        if Arc::strong_count(&self.files) == 1 {
            self.cleanup_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_manager() {
        let manager = TempFileManager::new();
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_create_temp_file() {
        let manager = TempFileManager::new();
        let path = manager.create_temp_file("test", "txt").unwrap();

        assert!(path.exists());
        assert!(path.to_string_lossy().contains("test"));
        assert!(path.to_string_lossy().ends_with(".txt"));
        assert_eq!(manager.count(), 1);

        // Cleanup
        manager.cleanup_all();
    }

    #[test]
    fn test_unique_filenames_three_files() {
        let manager = TempFileManager::new();

        let path1 = manager.create_temp_file("screenshot", "png").unwrap();
        let path2 = manager.create_temp_file("screenshot", "png").unwrap();
        let path3 = manager.create_temp_file("screenshot", "png").unwrap();

        // All paths should be different
        assert_ne!(path1, path2);
        assert_ne!(path2, path3);
        assert_ne!(path1, path3);

        // All should exist
        assert!(path1.exists());
        assert!(path2.exists());
        assert!(path3.exists());

        // Manager should track all three
        assert_eq!(manager.count(), 3);

        // Cleanup
        manager.cleanup_all();
    }

    #[test]
    fn test_cleanup_all() {
        let manager = TempFileManager::new();

        let path1 = manager.create_temp_file("test", "png").unwrap();
        let path2 = manager.create_temp_file("test", "jpg").unwrap();

        assert!(path1.exists());
        assert!(path2.exists());
        assert_eq!(manager.count(), 2);

        // Cleanup
        manager.cleanup_all();

        // Files should be deleted
        assert!(!path1.exists());
        assert!(!path2.exists());
        assert_eq!(manager.count(), 0);
    }

    #[test]
    fn test_cleanup_on_drop() {
        let path = {
            let manager = TempFileManager::new();
            let path = manager.create_temp_file("drop_test", "txt").unwrap();
            assert!(path.exists());
            path
        }; // manager dropped here

        // File should be deleted after manager is dropped
        assert!(!path.exists());
    }

    #[test]
    fn test_write_image_png() {
        let manager = TempFileManager::new();
        let data = vec![0u8, 1, 2, 3, 4, 5];

        let (path, size) = manager.write_image(&data, ImageFormat::Png).unwrap();

        assert!(path.exists());
        assert_eq!(size, 6);
        assert!(path.to_string_lossy().ends_with(".png"));

        // Verify content
        let content = fs::read(&path).unwrap();
        assert_eq!(content, data);

        // Cleanup
        manager.cleanup_all();
    }

    #[test]
    fn test_write_image_jpeg() {
        let manager = TempFileManager::new();
        let data = vec![0xff, 0xd8, 0xff, 0xe0]; // JPEG header

        let (path, size) = manager.write_image(&data, ImageFormat::Jpeg).unwrap();

        assert!(path.exists());
        assert_eq!(size, 4);
        assert!(path.to_string_lossy().ends_with(".jpg"));

        // Cleanup
        manager.cleanup_all();
    }

    #[test]
    fn test_write_image_webp() {
        let manager = TempFileManager::new();
        let data = b"RIFF....WEBP".to_vec();

        let (path, size) = manager.write_image(&data, ImageFormat::Webp).unwrap();

        assert!(path.exists());
        assert_eq!(size, 12);
        assert!(path.to_string_lossy().ends_with(".webp"));

        // Cleanup
        manager.cleanup_all();
    }

    #[test]
    fn test_thread_safety_with_arc_clone() {
        use std::{sync::Arc, thread};

        let manager = Arc::new(TempFileManager::new());
        let manager_clone = Arc::clone(&manager);

        let handle =
            thread::spawn(move || manager_clone.create_temp_file("thread", "txt").unwrap());

        let path1 = manager.create_temp_file("main", "txt").unwrap();
        let path2 = handle.join().unwrap();

        assert!(path1.exists());
        assert!(path2.exists());
        assert_ne!(path1, path2);

        // Both files should be tracked
        assert_eq!(manager.count(), 2);

        // Cleanup
        manager.cleanup_all();
    }

    #[test]
    fn test_list_files() {
        let manager = TempFileManager::new();

        assert_eq!(manager.list_files().len(), 0);

        let _path1 = manager.create_temp_file("test", "png").unwrap();
        let _path2 = manager.create_temp_file("test", "jpg").unwrap();

        let files = manager.list_files();
        assert_eq!(files.len(), 2);

        // Cleanup
        manager.cleanup_all();
    }

    #[test]
    fn test_temp_dir_creation() {
        let manager = TempFileManager::new();
        let _path = manager.create_temp_file("test", "txt").unwrap();

        // Verify the screenshot-mcp subdirectory was created
        let temp_dir = TempFileManager::temp_dir();
        assert!(temp_dir.exists());
        assert!(temp_dir.to_string_lossy().contains("screenshot-mcp"));

        // Cleanup
        manager.cleanup_all();
    }

    #[test]
    fn test_multiple_managers_independent() {
        let manager1 = TempFileManager::new();
        let manager2 = TempFileManager::new();

        let _path1 = manager1.create_temp_file("mgr1", "txt").unwrap();
        let _path2 = manager2.create_temp_file("mgr2", "txt").unwrap();

        // Each manager tracks its own files
        assert_eq!(manager1.count(), 1);
        assert_eq!(manager2.count(), 1);

        // Cleanup one doesn't affect the other
        manager1.cleanup_all();
        assert_eq!(manager1.count(), 0);
        assert_eq!(manager2.count(), 1);

        // Cleanup remaining
        manager2.cleanup_all();
    }

    #[test]
    fn test_clone_shares_state() {
        let manager1 = TempFileManager::new();
        let manager2 = manager1.clone();

        let _path1 = manager1.create_temp_file("test", "txt").unwrap();

        // Cloned manager sees the same files
        assert_eq!(manager1.count(), 1);
        assert_eq!(manager2.count(), 1);

        let _path2 = manager2.create_temp_file("test2", "txt").unwrap();

        // Both see both files
        assert_eq!(manager1.count(), 2);
        assert_eq!(manager2.count(), 2);

        // Cleanup via either manager works
        manager1.cleanup_all();
        assert_eq!(manager1.count(), 0);
        assert_eq!(manager2.count(), 0);
    }

    #[test]
    fn test_default_trait() {
        let manager = TempFileManager::default();
        assert_eq!(manager.count(), 0);
    }
}
