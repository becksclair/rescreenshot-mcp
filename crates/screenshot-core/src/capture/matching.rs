//! Shared window matching strategies
//!
//! Platform-agnostic window matching logic used by X11 and Windows backends.
//! Provides a unified `WindowMatcher` that implements AND semantics: when
//! multiple fields are specified in a `WindowSelector`, all must match.
//!
//! # Matching Strategies
//!
//! For title matching, the matcher tries multiple strategies in order:
//! 1. **Regex match** - Case-insensitive regex on window title
//! 2. **Substring match** - Case-insensitive substring search on title
//! 3. **Fuzzy match** - Fuzzy matching on title using SkimMatcherV2
//!
//! For class and exe matching:
//! - **Exact match** - Case-insensitive exact match
//!
//! # AND Semantics
//!
//! When multiple fields are specified in `WindowSelector`, all must match:
//! - `title="Firefox"` AND `class="Navigator"` → window must match both
//! - `title="Code"` AND `exe="code"` → window must match both
//!
//! # Security
//!
//! Regex patterns are limited to 1MB to prevent ReDoS attacks.
//!
//! # Performance
//!
//! Compiled regex patterns are cached in a global LRU cache to avoid
//! recompilation when the same pattern is used repeatedly. The global cache
//! is more memory-efficient than thread-local caches in async/tokio contexts
//! where tasks migrate between threads.

use std::num::NonZeroUsize;

use once_cell::sync::Lazy;

use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use lru::LruCache;
use parking_lot::Mutex;
use regex::RegexBuilder;

use crate::model::{WindowHandle, WindowInfo, WindowSelector};

/// Maximum regex pattern size (1MB) to prevent ReDoS attacks
const MAX_REGEX_SIZE: usize = 1_048_576;

/// Maximum DFA size (10MB) to prevent ReDoS from complex patterns
const MAX_DFA_SIZE: usize = 10 * 1_048_576;

/// Minimum fuzzy match score for a positive match
const FUZZY_THRESHOLD: i64 = 60;

/// Maximum number of compiled regexes to cache globally.
///
/// Kept conservative (32) because complex patterns can have DFAs up to 10MB each.
const MAX_REGEX_CACHE_SIZE: usize = 32;

/// Global LRU cache for compiled regex patterns.
///
/// Uses LRU eviction to automatically remove least-recently-used patterns when
/// the cache is full. This is more memory-efficient than thread-local caches
/// in async/tokio contexts where tasks migrate between threads.
///
/// Cache entries are stored as `Option<Regex>` where `None` indicates a
/// pattern that failed validation (too large, DFA too complex, or invalid
/// syntax). This prevents repeated failed compilation attempts.
static REGEX_CACHE: Lazy<Mutex<LruCache<String, Option<regex::Regex>>>> =
    Lazy::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(MAX_REGEX_CACHE_SIZE).unwrap())));

/// Attempts to get or compile a regex pattern with caching and safety limits.
///
/// Returns `None` if:
/// - Pattern is too large (>1MB)
/// - Pattern compiles to a DFA that's too large (>10MB)
/// - Pattern is invalid regex syntax
///
/// Results (including failures) are cached to avoid repeated compilation.
/// Uses a global LRU cache with automatic eviction of least-recently-used
/// patterns, which is more efficient than thread-local caches in async contexts.
fn get_or_compile_regex(pattern: &str) -> Option<regex::Regex> {
    let mut cache = REGEX_CACHE.lock();

    // Check if pattern is already in cache (also updates LRU order)
    if let Some(cached) = cache.get(pattern) {
        return cached.clone();
    }

    // Compile the regex with safety checks
    let compiled = compile_regex_with_limits(pattern);

    // Cache the result (including None for failed compilations)
    // LRU eviction happens automatically when cache is full
    cache.put(pattern.to_string(), compiled.clone());

    compiled
}

/// Compiles a regex pattern with safety limits (no caching).
///
/// This is the core compilation logic used by the cache. ReDoS protection
/// relies on the DFA size limit (10MB) which accurately bounds complexity,
/// rather than naive character counting that produces false positives.
fn compile_regex_with_limits(pattern: &str) -> Option<regex::Regex> {
    // Limit pattern size to prevent ReDoS
    if pattern.len() > MAX_REGEX_SIZE {
        tracing::warn!("Regex pattern too large (>1MB), skipping regex match");
        return None;
    }

    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .size_limit(MAX_REGEX_SIZE)
        .dfa_size_limit(MAX_DFA_SIZE)
        .build()
        .ok()
}

#[derive(Debug, Clone, Copy)]
struct MatchScore {
    /// Title match strategy rank (higher is better)
    /// - 3: regex
    /// - 2: substring
    /// - 1: fuzzy
    /// - 0: no title criterion
    title_rank: u8,
    /// Fuzzy match score (only relevant when title_rank == 1)
    fuzzy_score: i64,
}

/// Unified window matcher implementing AND semantics
///
/// `WindowMatcher` is the single authority for window matching across all
/// backends. It enforces AND semantics: when multiple fields are specified
/// in a `WindowSelector`, all must match.
///
/// # Matching Order
///
/// For title matching, tries strategies in order:
/// 1. Regex match (if pattern is valid regex)
/// 2. Substring match (case-insensitive)
/// 3. Fuzzy match (threshold >= 60)
///
/// Class and exe matching use case-insensitive exact match.
///
/// # Examples
///
/// ```ignore
/// use screenshot_core::capture::matching::WindowMatcher;
/// use screenshot_core::model::WindowSelector;
///
/// let matcher = WindowMatcher::new();
/// let selector = WindowSelector {
///     title_substring_or_regex: Some("Firefox".to_string()),
///     class: Some("Navigator".to_string()),
///     exe: None,
/// };
///
/// // Find window matching both title AND class
/// if let Some(handle) = matcher.find_match(&selector, &windows) {
///     println!("Found: {}", handle);
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct WindowMatcher;

impl WindowMatcher {
    /// Creates a new WindowMatcher instance
    ///
    /// The matcher is stateless and can be reused across multiple matching
    /// operations.
    pub fn new() -> Self {
        Self
    }

    /// Finds the best window matching the selector using AND semantics
    ///
    /// When multiple fields are specified in the selector, all must match.
    /// Returns `None` if no window matches all specified criteria.
    ///
    /// If multiple windows match, the matcher applies deterministic tie-breaking:
    /// - Prefer stronger title match strategy: regex > substring > fuzzy
    /// - For fuzzy matches, prefer higher fuzzy score
    /// - As a final tie-breaker, prefer lexicographically smaller `WindowInfo.id`
    ///
    /// # Arguments
    ///
    /// - `selector` - Window selector with title/class/exe criteria
    /// - `windows` - List of windows to search
    ///
    /// # Returns
    ///
    /// - `Some(WindowHandle)` - First window matching all criteria
    /// - `None` - No window matches all criteria
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let matcher = WindowMatcher::new();
    /// let selector = WindowSelector {
    ///     title_substring_or_regex: Some("Firefox".to_string()),
    ///     class: Some("Navigator".to_string()),
    ///     exe: None,
    /// };
    ///
    /// // Only windows with title containing "Firefox" AND class "Navigator" match
    /// let handle = matcher.find_match(&selector, &windows);
    /// ```
    pub fn find_match(
        &self,
        selector: &WindowSelector,
        windows: &[WindowInfo],
    ) -> Option<WindowHandle> {
        // Validate selector has at least one criterion
        if selector.title_substring_or_regex.is_none()
            && selector.class.is_none()
            && selector.exe.is_none()
        {
            tracing::debug!("WindowMatcher: empty selector, no match");
            return None;
        }

        let title_pattern = selector.title_substring_or_regex.as_deref();

        let (title_regex, title_lower) = match title_pattern {
            Some(p) => (self.try_compile_regex(p), Some(p.to_lowercase())),
            None => (None, None),
        };

        let fuzzy_matcher = SkimMatcherV2::default();

        let mut best: Option<(&WindowInfo, MatchScore)> = None;

        for window in windows {
            let Some(score) = self.score_window(
                selector,
                window,
                title_pattern,
                title_lower.as_deref(),
                title_regex.as_ref(),
                &fuzzy_matcher,
            ) else {
                continue;
            };

            match best {
                None => best = Some((window, score)),
                Some((best_window, best_score)) => {
                    if Self::is_better_match(score, &window.id, best_score, &best_window.id) {
                        best = Some((window, score));
                    }
                }
            }
        }

        if let Some((window, _)) = best {
            tracing::debug!(
                "WindowMatcher: matched window {} (title: '{}', class: '{}', exe: '{}')",
                window.id,
                window.title,
                window.class,
                window.owner
            );
            return Some(window.id.clone());
        }

        tracing::debug!("WindowMatcher: no window matched all criteria");
        None
    }

    /// Checks if a single window matches all specified criteria (AND semantics).
    ///
    /// This is primarily intended for tests; production selection should use
    /// `find_match` to benefit from deterministic tie-breaking.
    ///
    /// Returns `true` only if the window matches ALL non-None fields in the
    /// selector.
    ///
    /// # Arguments
    ///
    /// - `selector` - Window selector with criteria
    /// - `window` - Window to check
    ///
    /// # Returns
    ///
    /// - `true` - Window matches all specified criteria
    /// - `false` - Window does not match one or more criteria
    #[cfg(test)]
    fn matches_window(&self, selector: &WindowSelector, window: &WindowInfo) -> bool {
        let title_pattern = selector.title_substring_or_regex.as_deref();
        let (title_regex, title_lower) = match title_pattern {
            Some(p) => (self.try_compile_regex(p), Some(p.to_lowercase())),
            None => (None, None),
        };
        let fuzzy_matcher = SkimMatcherV2::default();

        self.score_window(
            selector,
            window,
            title_pattern,
            title_lower.as_deref(),
            title_regex.as_ref(),
            &fuzzy_matcher,
        )
        .is_some()
    }

    fn score_window(
        &self,
        selector: &WindowSelector,
        window: &WindowInfo,
        title_pattern: Option<&str>,
        title_pattern_lower: Option<&str>,
        title_regex: Option<&regex::Regex>,
        fuzzy_matcher: &SkimMatcherV2,
    ) -> Option<MatchScore> {
        // Class match (if specified) - case-insensitive exact match
        if let Some(ref class) = selector.class {
            if !window.class.eq_ignore_ascii_case(class) {
                return None;
            }
        }

        // Exe match (if specified) - case-insensitive exact match
        if let Some(ref exe) = selector.exe {
            if !window.owner.eq_ignore_ascii_case(exe) {
                return None;
            }
        }

        // Title match (if specified)
        let title_score = match (title_pattern, title_pattern_lower) {
            (Some(pattern), Some(pattern_lower)) => {
                self.score_title(pattern, pattern_lower, title_regex, fuzzy_matcher, window)?
            }
            (None, None) => MatchScore {
                title_rank: 0,
                fuzzy_score: 0,
            },
            _ => {
                // Should be impossible: title_pattern and title_pattern_lower are built together.
                return None;
            }
        };

        Some(title_score)
    }

    fn score_title(
        &self,
        pattern: &str,
        pattern_lower: &str,
        compiled: Option<&regex::Regex>,
        fuzzy_matcher: &SkimMatcherV2,
        window: &WindowInfo,
    ) -> Option<MatchScore> {
        // Strategy 1: Regex match (strongest)
        if let Some(regex) = compiled {
            if regex.is_match(&window.title) {
                tracing::trace!("Title matched via regex: '{}'", window.title);
                return Some(MatchScore {
                    title_rank: 3,
                    fuzzy_score: 0,
                });
            }
        }

        // Strategy 2: Substring match
        if window.title.to_lowercase().contains(pattern_lower) {
            tracing::trace!("Title matched via substring: '{}'", window.title);
            return Some(MatchScore {
                title_rank: 2,
                fuzzy_score: 0,
            });
        }

        // Strategy 3: Fuzzy match (weakest)
        if let Some(score) = fuzzy_matcher.fuzzy_match(&window.title, pattern) {
            if score >= FUZZY_THRESHOLD {
                tracing::trace!(
                    "Fuzzy match: '{}' vs '{}' (score: {})",
                    pattern,
                    window.title,
                    score
                );
                return Some(MatchScore {
                    title_rank: 1,
                    fuzzy_score: score,
                });
            }
        }

        None
    }

    /// Attempts to compile a regex pattern with safety limits and caching.
    ///
    /// Uses thread-local caching to avoid recompilation of the same pattern.
    ///
    /// Returns `None` if:
    /// - Pattern is too large (>1MB)
    /// - Pattern has too many repetition operators (>10)
    /// - Pattern compiles to a DFA that's too large (>10MB)
    /// - Pattern is invalid regex syntax
    fn try_compile_regex(&self, pattern: &str) -> Option<regex::Regex> {
        get_or_compile_regex(pattern)
    }

    fn is_better_match(
        candidate: MatchScore,
        candidate_id: &str,
        best: MatchScore,
        best_id: &str,
    ) -> bool {
        // Primary: stronger title strategy
        if candidate.title_rank != best.title_rank {
            return candidate.title_rank > best.title_rank;
        }

        // Secondary: higher fuzzy score (only meaningful for fuzzy matches)
        if candidate.fuzzy_score != best.fuzzy_score {
            return candidate.fuzzy_score > best.fuzzy_score;
        }

        // Final: deterministic stable tie-breaker
        candidate_id < best_id
    }
}

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
/// use screenshot_core::capture::matching;
///
/// let windows = vec![/* ... */];
/// if let Some(handle) = matching::try_regex_match("Firefox.*", &windows) {
///     println!("Found Firefox: {}", handle);
/// }
/// ```
pub fn try_regex_match(pattern: &str, windows: &[WindowInfo]) -> Option<WindowHandle> {
    let regex = get_or_compile_regex(pattern)?;

    for window in windows {
        if regex.is_match(&window.title) {
            tracing::debug!("Regex matched window: {} (title: {})", window.id, window.title);
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
            tracing::debug!("Substring matched window: {} (title: {})", window.id, window.title);
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
            tracing::debug!("Class matched window: {} (class: {})", window.id, window.class);
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
            tracing::debug!("Exe matched window: {} (owner: {})", window.id, window.owner);
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
            id: id.to_string(),
            title: title.to_string(),
            class: class.to_string(),
            owner: owner.to_string(),
            pid: 1234,
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

    // ========== WindowMatcher AND Semantics Tests ==========

    #[test]
    fn test_window_matcher_new() {
        let matcher = WindowMatcher::new();
        // Should create successfully
        let _ = matcher;
    }

    #[test]
    fn test_window_matcher_find_match_by_title_only() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector::by_title("Firefox");

        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_window_matcher_find_match_by_class_only() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector::by_class("Code");

        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, Some("2".to_string()));
    }

    #[test]
    fn test_window_matcher_find_match_by_exe_only() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector::by_exe("firefox");

        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_window_matcher_and_semantics_title_and_class() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector {
            title_substring_or_regex: Some("Firefox".to_string()),
            class: Some("Navigator".to_string()),
            exe: None,
        };

        // Should match window 1 (Firefox with Navigator class)
        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, Some("1".to_string()));

        // Should NOT match window 2 (Code doesn't have Navigator class)
        let selector2 = WindowSelector {
            title_substring_or_regex: Some("Code".to_string()),
            class: Some("Navigator".to_string()),
            exe: None,
        };
        let result2 = matcher.find_match(&selector2, &windows);
        assert_eq!(result2, None);
    }

    #[test]
    fn test_window_matcher_and_semantics_title_and_exe() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector {
            title_substring_or_regex: Some("Visual Studio".to_string()),
            class: None,
            exe: Some("code".to_string()),
        };

        // Should match window 2 (VS Code with code exe)
        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, Some("2".to_string()));

        // Should NOT match if exe doesn't match
        let selector2 = WindowSelector {
            title_substring_or_regex: Some("Visual Studio".to_string()),
            class: None,
            exe: Some("firefox".to_string()),
        };
        let result2 = matcher.find_match(&selector2, &windows);
        assert_eq!(result2, None);
    }

    #[test]
    fn test_window_matcher_and_semantics_class_and_exe() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector {
            title_substring_or_regex: None,
            class: Some("Code".to_string()),
            exe: Some("code".to_string()),
        };

        // Should match window 2 (Code class and code exe)
        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, Some("2".to_string()));

        // Should NOT match if class doesn't match
        let selector2 = WindowSelector {
            title_substring_or_regex: None,
            class: Some("Navigator".to_string()),
            exe: Some("code".to_string()),
        };
        let result2 = matcher.find_match(&selector2, &windows);
        assert_eq!(result2, None);
    }

    #[test]
    fn test_window_matcher_and_semantics_all_three_fields() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector {
            title_substring_or_regex: Some("Visual Studio Code".to_string()),
            class: Some("Code".to_string()),
            exe: Some("code".to_string()),
        };

        // Should match window 2 (all three match)
        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, Some("2".to_string()));

        // Should NOT match if any field doesn't match
        let selector2 = WindowSelector {
            title_substring_or_regex: Some("Visual Studio Code".to_string()),
            class: Some("Code".to_string()),
            exe: Some("firefox".to_string()), // Wrong exe
        };
        let result2 = matcher.find_match(&selector2, &windows);
        assert_eq!(result2, None);
    }

    #[test]
    fn test_window_matcher_case_insensitive_class() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector {
            title_substring_or_regex: None,
            class: Some("CODE".to_string()), // Uppercase
            exe: None,
        };

        // Should match window 2 (case-insensitive class match)
        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, Some("2".to_string()));
    }

    #[test]
    fn test_window_matcher_case_insensitive_exe() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector {
            title_substring_or_regex: None,
            class: None,
            exe: Some("FIREFOX".to_string()), // Uppercase
        };

        // Should match window 1 (case-insensitive exe match)
        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_window_matcher_case_insensitive_title() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector::by_title("FIREFOX"); // Uppercase

        // Should match window 1 (case-insensitive title match)
        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, Some("1".to_string()));
    }

    #[test]
    fn test_window_matcher_regex_in_title() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector {
            title_substring_or_regex: Some("Visual.*Code".to_string()), // Regex pattern
            class: None,
            exe: None,
        };

        // Should match window 2 via regex
        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, Some("2".to_string()));
    }

    #[test]
    fn test_window_matcher_regex_fallback_to_substring() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        // Invalid regex should fallback to substring match
        let selector = WindowSelector {
            title_substring_or_regex: Some("[invalid(".to_string()), // Invalid regex
            class: None,
            exe: None,
        };

        // Should still match via substring fallback (if any window title contains "[invalid(")
        // In this case, none match, so should return None
        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, None);

        // But valid substring should work
        let selector2 = WindowSelector {
            title_substring_or_regex: Some("Studio".to_string()),
            class: None,
            exe: None,
        };
        let result2 = matcher.find_match(&selector2, &windows);
        assert_eq!(result2, Some("2".to_string()));
    }

    #[test]
    fn test_window_matcher_empty_selector() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector {
            title_substring_or_regex: None,
            class: None,
            exe: None,
        };

        // Empty selector should return None
        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, None);
    }

    #[test]
    fn test_window_matcher_no_match() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        let selector = WindowSelector::by_title("NonExistentWindow");

        // Should return None when no window matches
        let result = matcher.find_match(&selector, &windows);
        assert_eq!(result, None);
    }

    #[test]
    fn test_window_matcher_regex_size_limit() {
        let matcher = WindowMatcher::new();
        let windows = sample_windows();
        // Create pattern > 1MB
        let large_pattern = "a".repeat(1_000_001);
        let selector = WindowSelector {
            title_substring_or_regex: Some(large_pattern),
            class: None,
            exe: None,
        };

        // Should fallback to substring match (which will also fail for such a large pattern)
        // But should not panic or crash
        let result = matcher.find_match(&selector, &windows);
        // Result may be None (no match) but should not panic
        let _ = result;
    }

    #[test]
    fn test_window_matcher_matches_window_helper() {
        let matcher = WindowMatcher::new();
        let window = make_window("1", "Mozilla Firefox", "Navigator", "firefox");
        let selector = WindowSelector {
            title_substring_or_regex: Some("Firefox".to_string()),
            class: Some("Navigator".to_string()),
            exe: Some("firefox".to_string()),
        };

        // All criteria match
        assert!(matcher.matches_window(&selector, &window));

        // Title doesn't match
        let selector2 = WindowSelector {
            title_substring_or_regex: Some("Chrome".to_string()),
            class: Some("Navigator".to_string()),
            exe: Some("firefox".to_string()),
        };
        assert!(!matcher.matches_window(&selector2, &window));

        // Class doesn't match
        let selector3 = WindowSelector {
            title_substring_or_regex: Some("Firefox".to_string()),
            class: Some("Chrome".to_string()),
            exe: Some("firefox".to_string()),
        };
        assert!(!matcher.matches_window(&selector3, &window));

        // Exe doesn't match
        let selector4 = WindowSelector {
            title_substring_or_regex: Some("Firefox".to_string()),
            class: Some("Navigator".to_string()),
            exe: Some("chrome".to_string()),
        };
        assert!(!matcher.matches_window(&selector4, &window));
    }

    // ========== Regex Caching Tests ==========

    #[test]
    fn test_regex_cache_basic() {
        // Compile the same pattern twice - should use cache
        let pattern = "Firefox.*";
        let regex1 = get_or_compile_regex(pattern);
        let regex2 = get_or_compile_regex(pattern);

        assert!(regex1.is_some());
        assert!(regex2.is_some());
    }

    #[test]
    fn test_regex_cache_invalid_pattern() {
        // Invalid pattern should be cached as None
        let pattern = "[invalid(";
        let regex1 = get_or_compile_regex(pattern);
        let regex2 = get_or_compile_regex(pattern);

        assert!(regex1.is_none());
        assert!(regex2.is_none());
    }

    #[test]
    fn test_regex_cache_different_patterns() {
        // Different patterns should compile independently
        let regex1 = get_or_compile_regex("Firefox");
        let regex2 = get_or_compile_regex("Chrome");
        let regex3 = get_or_compile_regex("Visual.*Code");

        assert!(regex1.is_some());
        assert!(regex2.is_some());
        assert!(regex3.is_some());

        // They should all be different
        assert!(regex1.as_ref().unwrap().as_str() != regex2.as_ref().unwrap().as_str());
    }

    #[test]
    fn test_regex_cache_size_limit() {
        // Fill the cache with many patterns
        for i in 0..(MAX_REGEX_CACHE_SIZE + 10) {
            let pattern = format!("pattern_{}", i);
            let _ = get_or_compile_regex(&pattern);
        }

        // Cache should have cleared at some point and still work
        let regex = get_or_compile_regex("final_pattern");
        assert!(regex.is_some());
    }

    #[test]
    fn test_compile_regex_with_limits_valid() {
        let regex = compile_regex_with_limits("Firefox.*");
        assert!(regex.is_some());

        // Should be case-insensitive
        assert!(regex.as_ref().unwrap().is_match("firefox"));
        assert!(regex.as_ref().unwrap().is_match("FIREFOX"));
    }

    #[test]
    fn test_compile_regex_with_limits_many_repetitions_allowed() {
        // Previously rejected due to naive repetition counting, now allowed
        // because we rely on DFA size limit instead (which is more accurate)
        let pattern = "a+b+c+d+e+f+g+h+i+j+k+";
        let regex = compile_regex_with_limits(pattern);
        // This pattern is simple and compiles fine within DFA limits
        assert!(regex.is_some());
    }

    #[test]
    fn test_compile_regex_patterns_with_literal_special_chars() {
        // Patterns like "version-[0-9]+" should now compile (the '-' and '+'
        // were previously counted incorrectly as repetition operators)
        let regex = compile_regex_with_limits("version-[0-9]+");
        assert!(regex.is_some());

        let regex = compile_regex_with_limits("file-name-pattern*");
        assert!(regex.is_some());
    }
}
