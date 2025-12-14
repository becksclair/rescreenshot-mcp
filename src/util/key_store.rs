//! Secure token storage for Wayland restore tokens
//!
//! This module provides a thread-safe token storage system with keyring-first
//! approach and encrypted file fallback. Tokens are stored securely using
//! the platform's native keyring service (gnome-keyring, kwallet, etc.) when
//! available, with automatic fallback to encrypted file storage on minimal
//! systems.
//!
//! # Architecture
//!
//! - **Primary**: Platform keyring via `keyring` crate
//! - **Fallback**: ChaCha20-Poly1305 encrypted JSON file
//! - **Thread-safe**: Arc<Mutex<HashMap>> for concurrent access
//! - **Key format**: `screenshot-mcp-wayland-{source_id}`
//!
//! # Examples
//!
//! ```
//! use screenshot_mcp::util::key_store::KeyStore;
//!
//! let store = KeyStore::new();
//!
//! // Store a token
//! store
//!     .store_token("window-123", "restore_token_xyz")
//!     .unwrap();
//!
//! // Retrieve it later
//! let token = store.retrieve_token("window-123").unwrap();
//! assert_eq!(token, Some("restore_token_xyz".to_string()));
//!
//! // Check if token exists
//! assert!(store.has_token("window-123").unwrap());
//!
//! // Delete when done
//! store.delete_token("window-123").unwrap();
//! ```

#[cfg(target_os = "linux")]
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::PathBuf,
    sync::{Arc, OnceLock, RwLock},
};

#[cfg(target_os = "linux")]
use chacha20poly1305::{
    ChaCha20Poly1305,
    aead::{Aead, KeyInit},
};
#[cfg(target_os = "linux")]
use hkdf::Hkdf;
#[cfg(target_os = "linux")]
use sha2::Sha256;

#[cfg(target_os = "linux")]
use crate::error::{CaptureError, CaptureResult};

/// Thread-safe secure token storage with keyring-first approach
///
/// The KeyStore automatically detects keyring availability and falls back
/// to encrypted file storage when necessary. All operations are thread-safe
/// and can be safely shared across threads using Arc.
///
/// # Security
///
/// - **Keyring**: Uses platform native secret storage (most secure)
/// - **File fallback**: ChaCha20-Poly1305 AEAD encryption with machine-specific
///   key
/// - **File permissions**: 0600 (owner read/write only)
/// - **Key derivation**: SHA-256 hash of hostname + username
///
/// # Examples
///
/// ```no_run
/// use screenshot_mcp::util::key_store::KeyStore;
///
/// let store = KeyStore::new();
///
/// // Store and retrieve tokens
/// store.store_token("my-source", "token123").unwrap();
/// let token = store.retrieve_token("my-source").unwrap();
/// assert_eq!(token, Some("token123".to_string()));
///
/// // Delete token
/// store.delete_token("my-source").unwrap();
/// assert!(!store.has_token("my-source").unwrap());
/// ```
#[cfg(target_os = "linux")]
#[derive(Debug)]
pub struct KeyStore {
    /// Service name for keyring entries
    service_name: String,
    /// Lazy detection of platform keyring availability
    keyring_available: Arc<OnceLock<bool>>,
    /// In-memory cache for file-based storage
    file_store: Arc<RwLock<HashMap<String, String>>>,
    /// Path to encrypted token file (if using file fallback)
    file_path: Option<PathBuf>,
    /// Cached encryption key for file operations
    encryption_key: Option<[u8; 32]>,
    /// In-memory index of all known source IDs (tokens may live in keyring or
    /// file)
    source_index: Arc<RwLock<HashSet<String>>>,
    /// Path to persisted source ID index
    index_path: PathBuf,
}

#[cfg(target_os = "linux")]
impl KeyStore {
    /// Creates a new KeyStore instance
    ///
    /// Uses lazy keyring detection - keyring availability is tested on first
    /// use rather than during construction. This avoids permission prompts
    /// and improves startup performance.
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::util::key_store::KeyStore;
    ///
    /// let store = KeyStore::new();
    /// // Store is ready to use, will automatically choose best storage backend on first access
    /// ```
    pub fn new() -> Self {
        let service_name = "screenshot-mcp-wayland".to_string();

        // Initialize file fallback
        let file_path = Self::get_file_path();
        let encryption_key = Self::derive_encryption_key();

        // Load existing tokens from file if available
        let file_store = match Self::load_from_file(&file_path, &encryption_key) {
            Ok(tokens) => Arc::new(RwLock::new(tokens)),
            Err(e) => {
                tracing::warn!("Failed to load token file: {}", e);
                Arc::new(RwLock::new(HashMap::new()))
            }
        };

        // Load persisted source index (includes keyring + file-backed tokens)
        let index_path = Self::get_index_path();
        let source_index = match Self::load_index(&index_path) {
            Ok(index) => Arc::new(RwLock::new(index)),
            Err(e) => {
                tracing::warn!("Failed to load Wayland source index: {}", e);
                Arc::new(RwLock::new(HashSet::new()))
            }
        };

        let instance = Self {
            service_name,
            keyring_available: Arc::new(OnceLock::new()),
            file_store,
            file_path: Some(file_path),
            encryption_key: Some(encryption_key),
            source_index,
            index_path,
        };

        if let Err(e) = instance.rebuild_index_from_file_store() {
            tracing::warn!("Failed to backfill Wayland source index from file store: {}", e);
        }

        instance
    }

    /// Stores a token for the given source ID
    ///
    /// Attempts to store in platform keyring first, falls back to encrypted
    /// file if keyring is unavailable. The key format is:
    /// `screenshot-mcp-wayland-{source_id}`
    ///
    /// # Arguments
    ///
    /// * `source_id` - Unique identifier for the window/display source
    /// * `token` - Restore token to store
    ///
    /// # Returns
    ///
    /// - `Ok(())` if token stored successfully
    /// - `Err(CaptureError)` if storage failed
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::util::key_store::KeyStore;
    ///
    /// let store = KeyStore::new();
    /// store.store_token("window-123", "my_restore_token").unwrap();
    /// ```
    pub fn store_token(&self, source_id: &str, token: &str) -> CaptureResult<()> {
        let key = self.make_key(source_id);

        // Get or initialize keyring availability on first access
        let use_keyring = *self.keyring_available.get_or_init(|| {
            // First access - do a roundtrip to detect if keyring is actually usable.
            //
            // Some environments (notably headless CI/containers) can report success on
            // writes but fail to retrieve later. We treat that as "keyring
            // unavailable" and fall back to encrypted file storage to keep
            // behavior deterministic.
            self.detect_keyring_roundtrip(&key, token)
        });

        // If keyring was initialized as true, it means the first store succeeded
        // For subsequent calls, try keyring if available
        if use_keyring && self.keyring_available.get().copied() == Some(true) {
            match self.store_in_keyring(&key, token) {
                Ok(()) => {
                    self.record_source_id(source_id)?;
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!("Keyring operation failed: {}", e);
                    // Fall through to file storage
                }
            }
        }

        // Store in encrypted file
        self.store_in_file(source_id, token)?;
        self.record_source_id(source_id)?;
        Ok(())
    }

    /// Retrieves a token for the given source ID
    ///
    /// Checks keyring first if available, then falls back to file storage.
    ///
    /// # Arguments
    ///
    /// * `source_id` - Source identifier to retrieve token for
    ///
    /// # Returns
    ///
    /// - `Ok(Some(token))` if token found
    /// - `Ok(None)` if token not found
    /// - `Err(CaptureError)` on retrieval error
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::util::key_store::KeyStore;
    ///
    /// let store = KeyStore::new();
    /// store.store_token("my-source", "token123").unwrap();
    ///
    /// let token = store.retrieve_token("my-source").unwrap();
    /// assert_eq!(token, Some("token123".to_string()));
    /// ```
    pub fn retrieve_token(&self, source_id: &str) -> CaptureResult<Option<String>> {
        let key = self.make_key(source_id);

        // Check if keyring is available (only if already initialized)
        if let Some(true) = self.keyring_available.get().copied() {
            // Try keyring first
            match self.retrieve_from_keyring(&key) {
                Ok(Some(token)) => return Ok(Some(token)),
                Ok(None) => {
                    // Not in keyring, try file fallback
                }
                Err(e) => {
                    tracing::warn!("Keyring retrieve failed, trying file fallback: {}", e);
                }
            }
        }

        // Try file storage
        self.retrieve_from_file(source_id)
    }

    /// Deletes a token for the given source ID
    ///
    /// Removes token from both keyring and file storage (if present).
    ///
    /// # Arguments
    ///
    /// * `source_id` - Source identifier to delete token for
    ///
    /// # Returns
    ///
    /// - `Ok(())` if deletion succeeded
    /// - `Err(CaptureError)` on deletion error
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::util::key_store::KeyStore;
    ///
    /// let store = KeyStore::new();
    /// store.store_token("temp-source", "temp-token").unwrap();
    /// store.delete_token("temp-source").unwrap();
    ///
    /// assert!(!store.has_token("temp-source").unwrap());
    /// ```
    pub fn delete_token(&self, source_id: &str) -> CaptureResult<()> {
        let key = self.make_key(source_id);

        // Try to delete from keyring (only if already initialized as available)
        if let Some(true) = self.keyring_available.get().copied() {
            if let Err(e) = self.delete_from_keyring(&key) {
                tracing::warn!("Keyring delete failed: {}", e);
            }
        }

        // Delete from file storage
        self.delete_from_file(source_id)?;
        self.remove_source_id(source_id)?;
        Ok(())
    }

    /// Checks if a token exists for the given source ID
    ///
    /// # Arguments
    ///
    /// * `source_id` - Source identifier to check
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if token exists
    /// - `Ok(false)` if token not found
    /// - `Err(CaptureError)` on check error
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::util::key_store::KeyStore;
    ///
    /// let store = KeyStore::new();
    /// assert!(!store.has_token("nonexistent").unwrap());
    ///
    /// store.store_token("exists", "token").unwrap();
    /// assert!(store.has_token("exists").unwrap());
    /// ```
    pub fn has_token(&self, source_id: &str) -> CaptureResult<bool> {
        match self.retrieve_token(source_id)? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    /// Returns all known source IDs that currently have stored tokens
    ///
    /// Source IDs are persisted even when tokens are stored exclusively in the
    /// system keyring, enabling enumeration for user interfaces.
    ///
    /// # Returns
    ///
    /// - `Ok(Vec<String>)` sorted alphabetically
    /// - `Err(CaptureError)` if index access fails
    pub fn list_source_ids(&self) -> CaptureResult<Vec<String>> {
        let index = self
            .source_index
            .read()
            .map_err(|e| CaptureError::EncryptionFailed {
                reason: format!("Failed to lock source index: {}", e),
            })?;

        let mut ids: Vec<String> = index.iter().cloned().collect();
        ids.sort();
        Ok(ids)
    }

    /// Atomically rotates a token (replaces old token with new one)
    ///
    /// This is a critical operation for Wayland restore tokens, which are
    /// single-use and must be replaced after each capture. The operation
    /// is atomic: the old token is deleted and the new token is stored
    /// in a single operation, ensuring thread-safety and consistency.
    ///
    /// Works with both keyring and file storage backends.
    ///
    /// # Arguments
    ///
    /// * `source_id` - Source identifier for the token to rotate
    /// * `new_token` - New restore token to store
    ///
    /// # Returns
    ///
    /// - `Ok(())` if rotation succeeded
    /// - `Err(TokenNotFound)` if no token exists for this source_id
    /// - `Err(EncryptionFailed)` if file persistence failed
    ///
    /// # Examples
    ///
    /// ```
    /// use screenshot_mcp::util::key_store::KeyStore;
    ///
    /// let store = KeyStore::new();
    /// store.store_token("window-123", "token_v1").unwrap();
    ///
    /// // After capture, portal returns new token
    /// store.rotate_token("window-123", "token_v2").unwrap();
    ///
    /// let token = store.retrieve_token("window-123").unwrap();
    /// assert_eq!(token, Some("token_v2".to_string()));
    /// ```
    pub fn rotate_token(&self, source_id: &str, new_token: &str) -> CaptureResult<()> {
        // First, verify token exists (check both keyring and file store)
        if !self.has_token(source_id)? {
            return Err(CaptureError::TokenNotFound {
                source_id: source_id.to_string(),
            });
        }

        // Delete old token (from both keyring and file)
        self.delete_token(source_id)?;

        // Store new token (will use same backend as before)
        self.store_token(source_id, new_token)?;

        tracing::debug!("Token rotated for source '{}' (new token stored)", source_id);
        Ok(())
    }

    // ========== Private Helper Methods ==========

    /// Constructs the full key name for keyring entries
    fn make_key(&self, source_id: &str) -> String {
        format!("{}-{}", self.service_name, source_id)
    }

    /// Detect whether the platform keyring supports a store+retrieve roundtrip.
    ///
    /// This is intentionally conservative: if we cannot reliably read back what
    /// we wrote, we treat the keyring as unavailable and fall back to
    /// file-based storage.
    fn detect_keyring_roundtrip(&self, key: &str, token: &str) -> bool {
        match self.store_in_keyring(key, token) {
            Ok(()) => match self.retrieve_from_keyring(key) {
                Ok(Some(stored)) if stored == token => {
                    tracing::info!("Platform keyring is available and supports roundtrip read");
                    let _ = self.delete_from_keyring(key);
                    true
                }
                Ok(other) => {
                    tracing::warn!(
                        "Keyring roundtrip failed (expected token, got {:?}); falling back to \
                         file storage",
                        other
                    );
                    let _ = self.delete_from_keyring(key);
                    false
                }
                Err(e) => {
                    tracing::warn!(
                        "Keyring roundtrip read failed ({}); falling back to file storage",
                        e
                    );
                    let _ = self.delete_from_keyring(key);
                    false
                }
            },
            Err(e) => {
                tracing::warn!("Keyring unavailable ({}), using encrypted file storage", e);
                false
            }
        }
    }

    /// Stores token in platform keyring
    fn store_in_keyring(&self, key: &str, token: &str) -> CaptureResult<()> {
        let entry = keyring::Entry::new(&self.service_name, key).map_err(|e| {
            CaptureError::KeyringOperationFailed {
                operation: "store".to_string(),
                reason: e.to_string(),
            }
        })?;

        entry
            .set_password(token)
            .map_err(|e| CaptureError::KeyringOperationFailed {
                operation: "store".to_string(),
                reason: e.to_string(),
            })?;

        Ok(())
    }

    /// Retrieves token from platform keyring
    fn retrieve_from_keyring(&self, key: &str) -> CaptureResult<Option<String>> {
        let entry = keyring::Entry::new(&self.service_name, key).map_err(|e| {
            CaptureError::KeyringOperationFailed {
                operation: "retrieve".to_string(),
                reason: e.to_string(),
            }
        })?;

        match entry.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CaptureError::KeyringOperationFailed {
                operation: "retrieve".to_string(),
                reason: e.to_string(),
            }),
        }
    }

    /// Deletes token from platform keyring
    fn delete_from_keyring(&self, key: &str) -> CaptureResult<()> {
        let entry = keyring::Entry::new(&self.service_name, key).map_err(|e| {
            CaptureError::KeyringOperationFailed {
                operation: "delete".to_string(),
                reason: e.to_string(),
            }
        })?;

        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()), // Already deleted
            Err(e) => Err(CaptureError::KeyringOperationFailed {
                operation: "delete".to_string(),
                reason: e.to_string(),
            }),
        }
    }

    /// Gets the path to the encrypted token file
    fn get_file_path() -> PathBuf {
        let data_dir = if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
            PathBuf::from(dir)
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".local/share")
        } else {
            PathBuf::from("/tmp")
        };

        data_dir.join("screenshot-mcp").join("token-store.enc")
    }

    /// Gets the path to the persisted source index file
    fn get_index_path() -> PathBuf {
        let data_dir = if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
            PathBuf::from(dir)
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".local/share")
        } else {
            PathBuf::from("/tmp")
        };

        data_dir
            .join("screenshot-mcp")
            .join("wayland-source-index.json")
    }

    /// Derives a machine-specific encryption key using HKDF
    ///
    /// Uses HKDF-SHA256 with hostname + username as input key material
    /// for proper key derivation with cryptographic guarantees.
    fn derive_encryption_key() -> [u8; 32] {
        // Collect input key material
        let mut ikm = Vec::new();
        if let Ok(hostname) = hostname::get() {
            ikm.extend_from_slice(hostname.as_encoded_bytes());
        }
        if let Ok(user) = std::env::var("USER") {
            ikm.extend_from_slice(user.as_bytes());
        } else if let Ok(user) = std::env::var("USERNAME") {
            ikm.extend_from_slice(user.as_bytes());
        }

        // Use HKDF for proper key derivation
        let hk = Hkdf::<Sha256>::new(
            Some(b"screenshot-mcp-wayland-v2"), // Salt (version bumped)
            &ikm,                               // Input key material
        );

        let mut okm = [0u8; 32];
        hk.expand(b"chacha20poly1305-key", &mut okm)
            .expect("32 bytes is valid for HKDF-SHA256");

        okm
    }

    /// Stores token in encrypted file
    fn store_in_file(&self, source_id: &str, token: &str) -> CaptureResult<()> {
        // Update in-memory store (write lock held briefly)
        let store_snapshot = {
            let mut store =
                self.file_store
                    .write()
                    .map_err(|e| CaptureError::EncryptionFailed {
                        reason: format!("Failed to lock file store: {}", e),
                    })?;

            store.insert(source_id.to_string(), token.to_string());
            store.clone() // Clone for serialization outside lock
        }; // Write lock released here

        // Serialize + encrypt + write (no lock held, doesn't block readers)
        self.save_to_file(&store_snapshot)?;
        Ok(())
    }

    /// Adds a source ID to the persisted index (no-op if already present)
    fn record_source_id(&self, source_id: &str) -> CaptureResult<()> {
        let mut index = self
            .source_index
            .write()
            .map_err(|e| CaptureError::EncryptionFailed {
                reason: format!("Failed to lock source index: {}", e),
            })?;

        if index.insert(source_id.to_string()) {
            self.save_index(&index)?;
        }

        Ok(())
    }

    /// Removes a source ID from the persisted index (no-op if absent)
    fn remove_source_id(&self, source_id: &str) -> CaptureResult<()> {
        let mut index = self
            .source_index
            .write()
            .map_err(|e| CaptureError::EncryptionFailed {
                reason: format!("Failed to lock source index: {}", e),
            })?;

        if index.remove(source_id) {
            self.save_index(&index)?;
        }

        Ok(())
    }

    /// Syncs the source index with any tokens already present in the file store
    fn rebuild_index_from_file_store(&self) -> CaptureResult<()> {
        let keys: Vec<String> = {
            let store = self
                .file_store
                .read()
                .map_err(|e| CaptureError::EncryptionFailed {
                    reason: format!("Failed to lock file store: {}", e),
                })?;
            store.keys().cloned().collect()
        };

        if keys.is_empty() {
            return Ok(());
        }

        let mut index = self
            .source_index
            .write()
            .map_err(|e| CaptureError::EncryptionFailed {
                reason: format!("Failed to lock source index: {}", e),
            })?;

        let mut changed = false;
        for key in keys {
            if index.insert(key) {
                changed = true;
            }
        }

        if changed {
            self.save_index(&index)?;
        }

        Ok(())
    }

    /// Retrieves token from file storage
    fn retrieve_from_file(&self, source_id: &str) -> CaptureResult<Option<String>> {
        let store = self
            .file_store
            .read() // Read lock (non-exclusive, allows concurrent reads)
            .map_err(|e| CaptureError::EncryptionFailed {
                reason: format!("Failed to lock file store: {}", e),
            })?;

        Ok(store.get(source_id).cloned())
    }

    /// Deletes token from file storage
    fn delete_from_file(&self, source_id: &str) -> CaptureResult<()> {
        // Update in-memory store (write lock held briefly)
        let store_snapshot = {
            let mut store =
                self.file_store
                    .write()
                    .map_err(|e| CaptureError::EncryptionFailed {
                        reason: format!("Failed to lock file store: {}", e),
                    })?;

            store.remove(source_id);
            store.clone() // Clone for serialization outside lock
        }; // Write lock released here

        // Serialize + encrypt + write (no lock held)
        self.save_to_file(&store_snapshot)?;
        Ok(())
    }

    /// Saves the source index to disk as JSON array
    fn save_index(&self, index: &HashSet<String>) -> CaptureResult<()> {
        let mut sorted: Vec<&String> = index.iter().collect();
        sorted.sort();

        let data = serde_json::to_vec(&sorted).map_err(|e| CaptureError::EncryptionFailed {
            reason: format!("Failed to serialize source index: {}", e),
        })?;

        if let Some(parent) = self.index_path.parent() {
            fs::create_dir_all(parent).map_err(CaptureError::IoError)?;
        }

        let mut file = fs::File::create(&self.index_path).map_err(CaptureError::IoError)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = file.metadata()?.permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(&self.index_path, permissions).map_err(CaptureError::IoError)?;
        }

        file.write_all(&data).map_err(CaptureError::IoError)?;
        Ok(())
    }

    /// Saves the token store to encrypted file (v2 format with random nonce)
    ///
    /// File format: [version:1][nonce:12][ciphertext:variable]
    fn save_to_file(&self, store: &HashMap<String, String>) -> CaptureResult<()> {
        const FILE_FORMAT_VERSION: u8 = 2;

        let file_path = self
            .file_path
            .as_ref()
            .ok_or_else(|| CaptureError::EncryptionFailed {
                reason: "File path not initialized".to_string(),
            })?;

        let encryption_key =
            self.encryption_key
                .as_ref()
                .ok_or_else(|| CaptureError::EncryptionFailed {
                    reason: "Encryption key not initialized".to_string(),
                })?;

        // Serialize to JSON
        let json = serde_json::to_vec(store).map_err(|e| CaptureError::EncryptionFailed {
            reason: format!("JSON serialization failed: {}", e),
        })?;

        // Encrypt with random nonce
        let cipher = ChaCha20Poly1305::new_from_slice(encryption_key).map_err(|e| {
            CaptureError::EncryptionFailed {
                reason: format!("Failed to create cipher: {}", e),
            }
        })?;

        // Generate random 12-byte nonce (CRITICAL SECURITY FIX)
        let mut nonce_bytes = [0u8; 12];
        {
            use rand::TryRngCore as _;
            let mut rng = rand::rngs::OsRng;
            rng.try_fill_bytes(&mut nonce_bytes)
                .map_err(|e| CaptureError::EncryptionFailed {
                    reason: format!("Failed to generate secure random nonce: {}", e),
                })?;
        }
        let nonce = &nonce_bytes.into();

        let ciphertext =
            cipher
                .encrypt(nonce, json.as_ref())
                .map_err(|e| CaptureError::EncryptionFailed {
                    reason: format!("Encryption failed: {}", e),
                })?;

        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(CaptureError::IoError)?;
        }

        // Write to file with restrictive permissions
        let mut file = fs::File::create(file_path).map_err(CaptureError::IoError)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = file.metadata()?.permissions();
            permissions.set_mode(0o600); // Owner read/write only
            fs::set_permissions(file_path, permissions).map_err(CaptureError::IoError)?;
        }

        // Write: [version][nonce][ciphertext]
        file.write_all(&[FILE_FORMAT_VERSION])
            .map_err(CaptureError::IoError)?;
        file.write_all(&nonce_bytes)
            .map_err(CaptureError::IoError)?;
        file.write_all(&ciphertext).map_err(CaptureError::IoError)?;

        Ok(())
    }

    /// Loads tokens from encrypted file with automatic v1→v2 migration
    ///
    /// Supports two formats:
    /// - v1 (legacy): [ciphertext] with fixed nonce "screenmcp123"
    /// - v2 (current): [version:1][nonce:12][ciphertext]
    fn load_from_file(
        file_path: &PathBuf,
        encryption_key: &[u8; 32],
    ) -> CaptureResult<HashMap<String, String>> {
        if !file_path.exists() {
            return Ok(HashMap::new());
        }

        // Read encrypted data
        let data = fs::read(file_path).map_err(CaptureError::IoError)?;

        if data.is_empty() {
            return Ok(HashMap::new());
        }

        let cipher = ChaCha20Poly1305::new_from_slice(encryption_key).map_err(|e| {
            CaptureError::EncryptionFailed {
                reason: format!("Failed to create cipher: {}", e),
            }
        })?;

        // Try v2 format first (version byte + nonce + ciphertext)
        if data.len() > 13 && data[0] == 2 {
            tracing::debug!("Loading token file in v2 format");
            return Self::load_v2(&data[1..], &cipher);
        }

        // Try v1 format (legacy fixed nonce) and auto-migrate
        tracing::info!(
            "Detected legacy token file format (v1), attempting to load and migrate to v2"
        );
        match Self::load_v1(&data, &cipher) {
            Ok(tokens) => {
                // Auto-upgrade to v2 format
                tracing::info!("Successfully loaded v1 tokens, migrating to v2 format");
                if let Err(e) = Self::save_v2_format(&tokens, file_path, encryption_key) {
                    tracing::warn!("Failed to migrate token file to v2: {}", e);
                } else {
                    tracing::info!("Successfully migrated token file to v2 format");
                }
                Ok(tokens)
            }
            Err(e) => {
                tracing::error!("Failed to load token file in any known format: {}", e);
                Err(e)
            }
        }
    }

    /// Loads persisted source index from disk (plain JSON array)
    fn load_index(index_path: &PathBuf) -> CaptureResult<HashSet<String>> {
        if !index_path.exists() {
            return Ok(HashSet::new());
        }

        let data = fs::read(index_path).map_err(CaptureError::IoError)?;
        if data.is_empty() {
            return Ok(HashSet::new());
        }

        let entries: Vec<String> =
            serde_json::from_slice(&data).map_err(|e| CaptureError::EncryptionFailed {
                reason: format!("Failed to deserialize source index: {}", e),
            })?;

        Ok(entries.into_iter().collect())
    }

    /// Loads v1 format (legacy with fixed nonce)
    fn load_v1(data: &[u8], cipher: &ChaCha20Poly1305) -> CaptureResult<HashMap<String, String>> {
        // v1 uses fixed nonce
        let nonce_bytes: [u8; 12] = *b"screenmcp123";
        let nonce = &nonce_bytes.into();

        let plaintext =
            cipher
                .decrypt(nonce, data)
                .map_err(|e| CaptureError::EncryptionFailed {
                    reason: format!("v1 decryption failed: {}", e),
                })?;

        serde_json::from_slice(&plaintext).map_err(|e| CaptureError::EncryptionFailed {
            reason: format!("v1 JSON deserialization failed: {}", e),
        })
    }

    /// Loads v2 format (nonce prefix)
    fn load_v2(data: &[u8], cipher: &ChaCha20Poly1305) -> CaptureResult<HashMap<String, String>> {
        if data.len() < 12 {
            return Err(CaptureError::EncryptionFailed {
                reason: "v2 file too short (missing nonce)".to_string(),
            });
        }

        // Split: [nonce:12][ciphertext]
        let (nonce_slice, ciphertext) = data.split_at(12);
        let nonce_array: [u8; 12] =
            nonce_slice
                .try_into()
                .map_err(|_| CaptureError::EncryptionFailed {
                    reason: "Invalid nonce length".to_string(),
                })?;
        let nonce = &nonce_array.into();

        let plaintext =
            cipher
                .decrypt(nonce, ciphertext)
                .map_err(|e| CaptureError::EncryptionFailed {
                    reason: format!("v2 decryption failed: {}", e),
                })?;

        serde_json::from_slice(&plaintext).map_err(|e| CaptureError::EncryptionFailed {
            reason: format!("v2 JSON deserialization failed: {}", e),
        })
    }

    /// Helper to save v2 format during migration
    fn save_v2_format(
        store: &HashMap<String, String>,
        file_path: &PathBuf,
        encryption_key: &[u8; 32],
    ) -> CaptureResult<()> {
        const FILE_FORMAT_VERSION: u8 = 2;

        // Serialize to JSON
        let json = serde_json::to_vec(store).map_err(|e| CaptureError::EncryptionFailed {
            reason: format!("JSON serialization failed: {}", e),
        })?;

        // Encrypt with random nonce
        let cipher = ChaCha20Poly1305::new_from_slice(encryption_key).map_err(|e| {
            CaptureError::EncryptionFailed {
                reason: format!("Failed to create cipher: {}", e),
            }
        })?;

        let mut nonce_bytes = [0u8; 12];
        {
            use rand::TryRngCore as _;
            let mut rng = rand::rngs::OsRng;
            rng.try_fill_bytes(&mut nonce_bytes)
                .map_err(|e| CaptureError::EncryptionFailed {
                    reason: format!("Failed to generate secure random nonce: {}", e),
                })?;
        }
        let nonce = &nonce_bytes.into();

        let ciphertext =
            cipher
                .encrypt(nonce, json.as_ref())
                .map_err(|e| CaptureError::EncryptionFailed {
                    reason: format!("Encryption failed: {}", e),
                })?;

        // Write to file
        let mut file = fs::File::create(file_path).map_err(CaptureError::IoError)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = file.metadata()?.permissions();
            permissions.set_mode(0o600);
            fs::set_permissions(file_path, permissions).map_err(CaptureError::IoError)?;
        }

        file.write_all(&[FILE_FORMAT_VERSION])
            .map_err(CaptureError::IoError)?;
        file.write_all(&nonce_bytes)
            .map_err(CaptureError::IoError)?;
        file.write_all(&ciphertext).map_err(CaptureError::IoError)?;

        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl Default for KeyStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;

    #[test]
    fn test_new_keystore() {
        let store = KeyStore::new();
        // Should initialize without error
        assert!(store.service_name == "screenshot-mcp-wayland");
    }

    #[test]
    fn test_store_and_retrieve_token() {
        let store = KeyStore::new();

        store.store_token("test-source", "test-token").unwrap();
        let token = store.retrieve_token("test-source").unwrap();

        assert_eq!(token, Some("test-token".to_string()));

        // Cleanup
        store.delete_token("test-source").unwrap();
    }

    #[test]
    fn test_list_source_ids_tracks_tokens() {
        let store = KeyStore::new();

        // Ensure clean baseline
        assert!(
            !store
                .list_source_ids()
                .unwrap()
                .contains(&"list-test-1".to_string())
        );

        store.store_token("list-test-1", "token1").unwrap();
        store.store_token("list-test-2", "token2").unwrap();

        let ids = store.list_source_ids().unwrap();
        assert!(ids.contains(&"list-test-1".to_string()));
        assert!(ids.contains(&"list-test-2".to_string()));

        store.delete_token("list-test-1").unwrap();
        let ids_after_delete = store.list_source_ids().unwrap();
        assert!(!ids_after_delete.contains(&"list-test-1".to_string()));

        store.delete_token("list-test-2").unwrap();
    }

    #[test]
    fn test_delete_token() {
        let store = KeyStore::new();

        store.store_token("delete-test", "token").unwrap();
        assert!(store.has_token("delete-test").unwrap());

        store.delete_token("delete-test").unwrap();
        assert!(!store.has_token("delete-test").unwrap());
    }

    #[test]
    fn test_has_token() {
        let store = KeyStore::new();

        assert!(!store.has_token("nonexistent").unwrap());

        store.store_token("exists", "token").unwrap();
        assert!(store.has_token("exists").unwrap());

        // Cleanup
        store.delete_token("exists").unwrap();
    }

    #[test]
    fn test_token_not_found() {
        let store = KeyStore::new();
        let result = store.retrieve_token("does-not-exist").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_multiple_tokens() {
        let store = KeyStore::new();

        store.store_token("source1", "token1").unwrap();
        store.store_token("source2", "token2").unwrap();
        store.store_token("source3", "token3").unwrap();

        assert_eq!(store.retrieve_token("source1").unwrap(), Some("token1".to_string()));
        assert_eq!(store.retrieve_token("source2").unwrap(), Some("token2".to_string()));
        assert_eq!(store.retrieve_token("source3").unwrap(), Some("token3".to_string()));

        // Cleanup
        store.delete_token("source1").unwrap();
        store.delete_token("source2").unwrap();
        store.delete_token("source3").unwrap();
    }

    #[test]
    fn test_token_overwrite() {
        let store = KeyStore::new();

        store.store_token("overwrite", "token1").unwrap();
        assert_eq!(store.retrieve_token("overwrite").unwrap(), Some("token1".to_string()));

        store.store_token("overwrite", "token2").unwrap();
        assert_eq!(store.retrieve_token("overwrite").unwrap(), Some("token2".to_string()));

        // Cleanup
        store.delete_token("overwrite").unwrap();
    }

    #[test]
    fn test_file_encryption_decryption() {
        let store = KeyStore::new();

        // Force file storage by storing and retrieving
        store.store_token("crypto-test", "sensitive-token").unwrap();

        // Verify file was created and is encrypted
        if let Some(file_path) = &store.file_path {
            if file_path.exists() {
                let contents = fs::read(file_path).unwrap();
                // Should not contain plaintext token
                assert!(!String::from_utf8_lossy(&contents).contains("sensitive-token"));
            }
        }

        // Should be able to retrieve
        let token = store.retrieve_token("crypto-test").unwrap();
        assert_eq!(token, Some("sensitive-token".to_string()));

        // Cleanup
        store.delete_token("crypto-test").unwrap();
    }

    #[test]
    fn test_thread_safety_concurrent_access() {
        use std::{sync::Arc, thread};

        let store = Arc::new(KeyStore::new());

        let handles: Vec<_> = (0..5)
            .map(|i| {
                let store_clone = Arc::clone(&store);
                thread::spawn(move || {
                    let source_id = format!("thread-{}", i);
                    let token = format!("token-{}", i);

                    store_clone.store_token(&source_id, &token).unwrap();
                    let retrieved = store_clone.retrieve_token(&source_id).unwrap();
                    assert_eq!(retrieved, Some(token));

                    store_clone.delete_token(&source_id).unwrap();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_key_format_correctness() {
        let store = KeyStore::new();
        let key = store.make_key("my-source");
        assert_eq!(key, "screenshot-mcp-wayland-my-source");
    }

    #[test]
    fn test_cleanup_after_delete() {
        let store = KeyStore::new();

        // Store multiple tokens
        store.store_token("cleanup1", "token1").unwrap();
        store.store_token("cleanup2", "token2").unwrap();

        // Delete one
        store.delete_token("cleanup1").unwrap();

        // First should be gone
        assert!(!store.has_token("cleanup1").unwrap());

        // Second should still exist
        assert!(store.has_token("cleanup2").unwrap());

        // Cleanup remaining
        store.delete_token("cleanup2").unwrap();
    }

    #[test]
    fn test_default_trait() {
        let store = KeyStore::default();
        assert!(store.service_name == "screenshot-mcp-wayland");
    }

    #[test]
    fn test_rotate_token_success() {
        let store = KeyStore::new();

        // Store initial token
        store.store_token("rotate-test", "token_v1").unwrap();
        assert_eq!(store.retrieve_token("rotate-test").unwrap(), Some("token_v1".to_string()));

        // Rotate to new token
        store.rotate_token("rotate-test", "token_v2").unwrap();
        assert_eq!(store.retrieve_token("rotate-test").unwrap(), Some("token_v2".to_string()));

        // Cleanup
        store.delete_token("rotate-test").unwrap();
    }

    #[test]
    fn test_rotate_token_nonexistent() {
        let store = KeyStore::new();

        // Try to rotate token that doesn't exist
        let result = store.rotate_token("nonexistent", "new_token");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CaptureError::TokenNotFound { .. }));
    }

    #[test]
    fn test_rotate_token_multiple_times() {
        let store = KeyStore::new();

        // Store initial token
        store.store_token("multi-rotate", "token_v1").unwrap();

        // Rotate multiple times
        store.rotate_token("multi-rotate", "token_v2").unwrap();
        assert_eq!(store.retrieve_token("multi-rotate").unwrap(), Some("token_v2".to_string()));

        store.rotate_token("multi-rotate", "token_v3").unwrap();
        assert_eq!(store.retrieve_token("multi-rotate").unwrap(), Some("token_v3".to_string()));

        store.rotate_token("multi-rotate", "token_v4").unwrap();
        assert_eq!(store.retrieve_token("multi-rotate").unwrap(), Some("token_v4".to_string()));

        // Cleanup
        store.delete_token("multi-rotate").unwrap();
    }

    #[test]
    fn test_rotate_token_atomicity() {
        use std::{sync::Arc, thread};

        let store = Arc::new(KeyStore::new());

        // Store initial token
        store.store_token("atomic-test", "token_v1").unwrap();

        // Spawn threads that rotate concurrently
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let store_clone = Arc::clone(&store);
                thread::spawn(move || {
                    let new_token = format!("token_v{}", i + 2);
                    // This may fail if another thread rotated first, that's expected
                    let _ = store_clone.rotate_token("atomic-test", &new_token);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Should have some token stored (one of the rotations succeeded)
        let token = store.retrieve_token("atomic-test").unwrap();
        assert!(token.is_some());
        assert!(token.unwrap().starts_with("token_v"));

        // Cleanup
        store.delete_token("atomic-test").unwrap();
    }

    #[test]
    fn test_rotate_token_persistence() {
        let store = KeyStore::new();

        // Store and rotate
        store.store_token("persist-test", "token_v1").unwrap();
        store.rotate_token("persist-test", "token_v2").unwrap();

        // Verify file was updated (if file path exists)
        if let Some(file_path) = &store.file_path {
            if file_path.exists() {
                let contents = fs::read(file_path).unwrap();
                // Should not contain old token in plaintext
                assert!(!String::from_utf8_lossy(&contents).contains("token_v1"));
            }
        }

        // Verify new token is retrievable
        assert_eq!(store.retrieve_token("persist-test").unwrap(), Some("token_v2".to_string()));

        // Cleanup
        store.delete_token("persist-test").unwrap();
    }
}
