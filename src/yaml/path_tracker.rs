//! Path tracking through YAML AST for error reporting
//!
//! Provides efficient path tracking as we traverse the YAML AST during resolution.
//! Used to generate precise error locations like "Resources.MyBucket.Properties.BucketName".

use smallvec::SmallVec;

/// Path tracker using SmallVec optimized for typical AST depths (usually < 8 levels)
#[derive(Debug, Clone)]
pub struct PathTracker {
    /// Path segments through the AST
    segments: SmallVec<[String; 8]>,
    // TODO consider SmallVec<[Cow<'a, str>; 8]>,
}

impl PathTracker {
    /// Create a new empty path tracker with optimal default capacity
    pub fn new() -> Self {
        Self {
            segments: SmallVec::with_capacity(8), // Start with 8 - good balance for most cases
        }
    }

    /// Create path tracker with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            segments: SmallVec::with_capacity(capacity),
        }
    }

    /// Push a path segment
    #[inline]
    pub fn push(&mut self, segment: &str) {
        self.segments.push(segment.to_string());
    }

    /// Pop the last path segment
    #[inline]
    pub fn pop(&mut self) -> Option<String> {
        self.segments.pop()
    }

    /// Get current path as dot-separated string
    pub fn current_path(&self) -> String {
        self.segments.join(".")
    }

    /// Get path length
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Check if path is empty
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Get path segments as slice
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Clear all path segments
    pub fn clear(&mut self) {
        self.segments.clear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_tracker_basic_operations() {
        let mut tracker = PathTracker::new();
        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);
        assert_eq!(tracker.current_path(), "");

        tracker.push("Resources");
        assert_eq!(tracker.len(), 1);
        assert_eq!(tracker.current_path(), "Resources");

        tracker.push("MyBucket");
        assert_eq!(tracker.len(), 2);
        assert_eq!(tracker.current_path(), "Resources.MyBucket");

        tracker.push("Properties");
        assert_eq!(tracker.len(), 3);
        assert_eq!(tracker.current_path(), "Resources.MyBucket.Properties");

        assert_eq!(tracker.pop(), Some("Properties".to_string()));
        assert_eq!(tracker.len(), 2);
        assert_eq!(tracker.current_path(), "Resources.MyBucket");

        tracker.clear();
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_path_tracker_capacity() {
        let tracker = PathTracker::with_capacity(16);
        assert!(tracker.is_empty());
        // SmallVec doesn't expose capacity directly, but it's allocated
    }

    #[test]
    fn test_path_tracker_push_pop() {
        let mut tracker = PathTracker::new();
        
        tracker.push("a");
        tracker.push("b");
        tracker.push("c");
        
        assert_eq!(tracker.segments(), &["a", "b", "c"]);
        
        assert_eq!(tracker.pop(), Some("c".to_string()));
        assert_eq!(tracker.pop(), Some("b".to_string()));
        assert_eq!(tracker.pop(), Some("a".to_string()));
        assert_eq!(tracker.pop(), None);
    }

    #[test]
    fn test_path_tracker_array_index() {
        let mut tracker = PathTracker::new();
        
        tracker.push("items");
        tracker.push("[0]");
        tracker.push("name");
        
        assert_eq!(tracker.current_path(), "items.[0].name");
    }
}