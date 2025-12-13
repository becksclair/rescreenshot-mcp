//! Shared window matching strategies
//!
//! Platform-agnostic window matching logic used by X11 and Windows backends.
//! Provides five matching strategies with consistent behavior across platforms:
//!
//! 1. **Regex match** - Case-insensitive regex on window title
//! 2. **Substring match** - Case-insensitive substring search on title
//! 3. **Class match** - Exact class name match (case-insensitive)
//! 4. **Exe match** - Exact executable/owner name match (case-insensitive)
//! 5. **Fuzzy match** - Fuzzy matching on title using SkimMatcherV2
//!
//! # Security
//!
//! Regex patterns are limited to 1MB to prevent ReDoS attacks.

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use regex::RegexBuilder;

use crate::model::{WindowHandle, WindowInfo};

/// Maximum regex pattern size (1MB) to prevent ReDoS attacks
const MAX_REGEX_SIZE: usize = 1_048_576;

/// Minimum fuzzy match score for a positive match
const FUZZY_THRESHOLD: i64 = 60;

/// Tries to match windows by regex pattern on title
///
/// Returns the first window whose title matches the case-insensitive regex pattern.
/// Pattern size is limited to 1MB to prevent ReDoS attacks.
///
/// # Arguments
///
/// - `pattern` - Regex pattern to match against window titles
/// - `windows` - List of windows to search
///
/// # Returns
///
/// - `Some(WindowHandle)` - First matching window
/// - `None` - No match, invalid regex, or pattern too large
///
/// # Examples
///
/// ```ignore
/// use screenshot_mcp::capture::matching;
///
/// let windows = vec![/* ... */];
/// if let Some(handle) = matching::try_regex_match("Firefox.*", &windows) {
///     println!("Found Firefox: {}", handle);
/// }
/// ```
pub fn try_regex_match(pattern: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
    // Limit pattern size to prevent ReDoS
    if pattern.len() > MAX_REGEX_SIZE {
        tracing::warn!("Regex pattern too large (>1MB), skipping regex match");
        return None;
    }

    let regex = match RegexBuilder::new(pattern)
        .case_insensitive(true)
        .size_limit(MAX_REGEX_SIZE)
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("Pattern '{}' is not a valid regex: {}", pattern, e);
            return None;
        }
    };

    for window in windows {
        if regex.is_match(&window.title) {
            tracing::debug!(
                "Regex matched window: {} (title: {})",
                window.id,
                window.title
            );
            return Some(window.id.clone());
        }
    }

    None
}

/// Tries to match windows by case-insensitive substring in title
///
/// Returns the first window whose title contains the substring.
///
/// # Arguments
///
/// - `substring` - Substring to search for in window titles
/// - `windows` - List of windows to search
///
/// # Returns
///
/// - `Some(WindowHandle)` - First matching window
/// - `None` - No match
pub fn try_substring_match(substring: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
    let substring_lower = substring.to_lowercase();

    for window in windows {
        if window.title.to_lowercase().contains(&substring_lower) {
            tracing::debug!(
                "Substring matched window: {} (title: {})",
                window.id,
                window.title
            );
            return Some(window.id.clone());
        }
    }

    None
}

/// Tries to match windows by exact class name (case-insensitive)
///
/// Returns the first window with matching class name.
///
/// # Arguments
///
/// - `class` - Class name to match
/// - `windows` - List of windows to search
///
/// # Returns
///
/// - `Some(WindowHandle)` - First matching window
/// - `None` - No match
pub fn try_class_match(class: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
    for window in windows {
        if window.class.eq_ignore_ascii_case(class) {
            tracing::debug!(
                "Class matched window: {} (class: {})",
                window.id,
                window.class
            );
            return Some(window.id.clone());
        }
    }

    None
}

/// Tries to match windows by exact executable/owner name (case-insensitive)
///
/// The `owner` field in WindowInfo contains either:
/// - On X11: The WM_CLASS instance name (typically the executable name)
/// - On Windows: The process executable name
///
/// # Arguments
///
/// - `exe` - Executable/instance name to match
/// - `windows` - List of windows to search
///
/// # Returns
///
/// - `Some(WindowHandle)` - First matching window
/// - `None` - No match
pub fn try_exe_match(exe: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
    for window in windows {
        if window.owner.eq_ignore_ascii_case(exe) {
            tracing::debug!(
                "Exe matched window: {} (owner: {})",
                window.id,
                window.owner
            );
            return Some(window.id.clone());
        }
    }

    None
}

/// Tries to match windows using fuzzy matching on title
///
/// Uses SkimMatcherV2 with a threshold of 60. Returns the highest-scoring
/// match above the threshold.
///
/// # Arguments
///
/// - `pattern` - Pattern to fuzzy-match against window titles
/// - `windows` - List of windows to search
///
/// # Returns
///
/// - `Some(WindowHandle)` - Best fuzzy match (score >= 60)
/// - `None` - No match above threshold
pub fn try_fuzzy_match(pattern: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
    let matcher = SkimMatcherV2::default();

    let mut best_match: Option<(WindowHandle, i64)> = None;

    for window in windows {
        if let Some(score) = matcher.fuzzy_match(&window.title, pattern) {
            if score >= FUZZY_THRESHOLD {
                tracing::debug!(
                    "Fuzzy match candidate: {} (title: {}, score: {})",
                    window.id,
                    window.title,
                    score
                );

                // Keep highest-scoring match
                if best_match.as_ref().is_none_or(|(_, s)| score > *s) {
                    best_match = Some((window.id.clone(), score));
                }
            }
        }
    }

    if let Some((handle, score)) = best_match {
        tracing::debug!("Best fuzzy match: {} (score: {})", handle, score);
        Some(handle)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BackendType;

    fn make_window(id: &str, title: &str, class: &str, owner: &str) -> WindowInfo {
        WindowInfo {
            id:      id.to_string(),
            title:   title.to_string(),
            class:   class.to_string(),
            owner:   owner.to_string(),
            pid:     1234,
            backend: BackendType::None,
        }
    }

    fn sample_windows() -> Vec<WindowInfo> {
        vec![
            make_window("1", "Mozilla Firefox", "Navigator", "firefox"),
            make_window("2", "Visual Studio Code", "Code", "code"),
            make_window("3", "Terminal - bash", "Gnome-terminal", "gnome-terminal"),
            make_window("4", "Settings", "Settings", "gnome-settings"),
        ]
    }

    #[test]
    fn test_regex_match_simple() {
        let windows = sample_windows();
        let result = try_regex_match("Firefox", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_regex_match_pattern() {
        let windows = sample_windows();
        let result = try_regex_match("Visual.*Code", &windows);
        assert_eq!(result, Some("2".to_string()));
    }

    #[test]
    fn test_regex_match_case_insensitive() {
        let windows = sample_windows();
        let result = try_regex_match("firefox", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_regex_match_invalid_pattern() {
        let windows = sample_windows();
        let result = try_regex_match("[invalid(", &windows);
        assert_eq!(result, None);
    }

    #[test]
    fn test_regex_match_no_match() {
        let windows = sample_windows();
        let result = try_regex_match("Chrome", &windows);
        assert_eq!(result, None);
    }

    #[test]
    fn test_substring_match() {
        let windows = sample_windows();
        let result = try_substring_match("Studio", &windows);
        assert_eq!(result, Some("2".to_string()));
    }

    #[test]
    fn test_substring_match_case_insensitive() {
        let windows = sample_windows();
        let result = try_substring_match("TERMINAL", &windows);
        assert_eq!(result, Some("3".to_string()));
    }

    #[test]
    fn test_substring_match_no_match() {
        let windows = sample_windows();
        let result = try_substring_match("Notepad", &windows);
        assert_eq!(result, None);
    }

    #[test]
    fn test_class_match() {
        let windows = sample_windows();
        let result = try_class_match("Navigator", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_class_match_case_insensitive() {
        let windows = sample_windows();
        let result = try_class_match("code", &windows);
        assert_eq!(result, Some("2".to_string()));
    }

    #[test]
    fn test_class_match_no_match() {
        let windows = sample_windows();
        let result = try_class_match("NonExistent", &windows);
        assert_eq!(result, None);
    }

    #[test]
    fn test_exe_match() {
        let windows = sample_windows();
        let result = try_exe_match("firefox", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_exe_match_case_insensitive() {
        let windows = sample_windows();
        let result = try_exe_match("CODE", &windows);
        assert_eq!(result, Some("2".to_string()));
    }

    #[test]
    fn test_exe_match_no_match() {
        let windows = sample_windows();
        let result = try_exe_match("chrome", &windows);
        assert_eq!(result, None);
    }

    #[test]
    fn test_fuzzy_match() {
        let windows = sample_windows();
        // "Firefx" should fuzzy match "Mozilla Firefox"
        let result = try_fuzzy_match("Firefx", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_fuzzy_match_best_score() {
        let windows = sample_windows();
        // Should match Firefox better than Settings
        let result = try_fuzzy_match("fire", &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_fuzzy_match_no_match() {
        let windows = sample_windows();
        // Too different to match anything
        let result = try_fuzzy_match("zzzzzzz", &windows);
        assert_eq!(result, None);
    }

    #[test]
    fn test_empty_windows_list() {
        let windows: Vec<WindowInfo> = vec![];
        assert_eq!(try_regex_match("test", &windows), None);
        assert_eq!(try_substring_match("test", &windows), None);
        assert_eq!(try_class_match("test", &windows), None);
        assert_eq!(try_exe_match("test", &windows), None);
        assert_eq!(try_fuzzy_match("test", &windows), None);
    }
}
