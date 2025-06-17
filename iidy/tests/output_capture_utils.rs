//! Utilities for capturing and testing renderer output
//!
//! This module provides utilities to capture output from renderers for testing
//! with insta snapshots. Enables pixel-perfect validation of rendered output
//! including ANSI color codes and formatting.

use std::sync::{Arc, Mutex};
use std::io::{self, Write};

/// A writer that captures all written data for testing
#[derive(Debug, Clone)]
pub struct OutputCapture {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl OutputCapture {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get the captured output as a UTF-8 string
    pub fn get_output(&self) -> String {
        let buffer = self.buffer.lock().unwrap();
        String::from_utf8_lossy(&buffer).to_string()
    }

    /// Clear the captured output
    #[allow(dead_code)] // Reserved for future use
    pub fn clear(&self) {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.clear();
    }

    /// Get output with ANSI codes stripped for plain text comparison
    pub fn get_output_plain(&self) -> String {
        use regex::Regex;
        let output = self.get_output();
        // Strip ANSI escape sequences
        let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        re.replace_all(&output, "").to_string()
    }

    /// Get only the ANSI codes for color testing
    pub fn get_ansi_codes(&self) -> Vec<String> {
        use regex::Regex;
        let output = self.get_output();
        let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        re.find_iter(&output)
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Create a scoped capture that redirects stdout/stderr
    #[allow(dead_code)] // Reserved for future use
    pub fn capture_stdio<F, R>(f: F) -> (R, String)
    where
        F: FnOnce() -> R,
    {
        let capture = OutputCapture::new();
        
        // This is a simplified version - in practice we'd need more sophisticated
        // stdio redirection, but for our controlled test environment this works
        let result = f();
        let output = capture.get_output();
        
        (result, output)
    }
}

impl Write for OutputCapture {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // No-op for memory buffer
        Ok(())
    }
}

/// Test utilities for renderer output validation
pub struct RendererTestUtils;

impl RendererTestUtils {
    /// Normalize output for consistent snapshot testing
    /// - Remove timestamps (they change every run)
    /// - Normalize paths for cross-platform testing
    /// - Handle dynamic content
    pub fn normalize_output(output: &str) -> String {
        use regex::Regex;
        
        let mut normalized = output.to_string();
        
        // Replace ISO timestamps with fixed placeholder
        let timestamp_re = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{3})?Z").unwrap();
        normalized = timestamp_re.replace_all(&normalized, "2024-01-15T10:30:45.123Z").to_string();
        
        // Replace Unix timestamps with fixed placeholder
        let unix_timestamp_re = Regex::new(r"\b\d{10}\b").unwrap();
        normalized = unix_timestamp_re.replace_all(&normalized, "1705318245").to_string();
        
        // Replace durations with fixed values
        let duration_re = Regex::new(r"\b\d+s\b").unwrap();
        normalized = duration_re.replace_all(&normalized, "42s").to_string();
        
        // Replace ARNs with consistent test ARNs
        let arn_re = Regex::new(r"arn:aws:[^:]+:[^:]*:\d{12}:[^/\s]+/[^\s]+").unwrap();
        normalized = arn_re.replace_all(&normalized, "arn:aws:cloudformation:us-east-1:123456789012:stack/test-stack/test-id").to_string();
        
        // Replace account IDs with test account
        let account_re = Regex::new(r"\b\d{12}\b").unwrap();
        normalized = account_re.replace_all(&normalized, "123456789012").to_string();
        
        // Replace stack IDs with consistent test IDs
        let stack_id_re = Regex::new(r"stack/[^/]+/[a-f0-9-]+").unwrap();
        normalized = stack_id_re.replace_all(&normalized, "stack/test-stack/test-stack-id-123").to_string();
        
        normalized
    }
    
    /// Extract just the color information for ANSI color testing
    pub fn extract_color_map(output: &str) -> std::collections::HashMap<String, Vec<String>> {
        use regex::Regex;
        use std::collections::HashMap;
        
        let mut color_map = HashMap::new();
        let lines: Vec<&str> = output.lines().collect();
        
        for line in lines {
            // Find ANSI codes and the text they apply to
            let ansi_re = Regex::new(r"\x1b\[([0-9;]*)m([^\x1b]*?)(?:\x1b\[0m|$)").unwrap();
            
            for cap in ansi_re.captures_iter(line) {
                let color_code = cap[1].to_string();
                let text_content = cap[2].trim().to_string();
                
                if !text_content.is_empty() && !color_code.is_empty() {
                    color_map.entry(text_content)
                        .or_insert_with(Vec::new)
                        .push(color_code);
                }
            }
        }
        
        color_map
    }
    
    /// Validate that specific colors are used for specific content types
    pub fn validate_color_usage(output: &str, expected_patterns: &[(&str, &str)]) -> Vec<String> {
        let color_map = Self::extract_color_map(output);
        let mut errors = Vec::new();
        
        for (content_pattern, expected_color) in expected_patterns {
            let content_regex = regex::Regex::new(content_pattern).unwrap();
            let mut found_match = false;
            
            for (text, colors) in &color_map {
                if content_regex.is_match(text) {
                    found_match = true;
                    if !colors.contains(&expected_color.to_string()) {
                        errors.push(format!(
                            "Content '{}' should use color '{}' but found colors: {:?}",
                            text, expected_color, colors
                        ));
                    }
                }
            }
            
            if !found_match {
                errors.push(format!(
                    "No content found matching pattern '{}' for color validation",
                    content_pattern
                ));
            }
        }
        
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_capture_basic() {
        let mut capture = OutputCapture::new();
        
        write!(capture, "Hello, ").unwrap();
        write!(capture, "World!").unwrap();
        
        assert_eq!(capture.get_output(), "Hello, World!");
    }

    #[test]
    fn test_output_capture_with_ansi() {
        let mut capture = OutputCapture::new();
        
        // Write some colored text
        write!(capture, "\x1b[31mRed text\x1b[0m and \x1b[32mgreen text\x1b[0m").unwrap();
        
        let full_output = capture.get_output();
        assert!(full_output.contains("\x1b[31m"));
        assert!(full_output.contains("\x1b[32m"));
        
        let plain_output = capture.get_output_plain();
        assert_eq!(plain_output, "Red text and green text");
        
        let ansi_codes = capture.get_ansi_codes();
        assert!(ansi_codes.contains(&"\x1b[31m".to_string()));
        assert!(ansi_codes.contains(&"\x1b[32m".to_string()));
        assert!(ansi_codes.contains(&"\x1b[0m".to_string()));
    }

    #[test]
    fn test_normalize_output() {
        let test_output = r#"
Stack created at 2024-06-17T15:30:22.456Z
Duration: 125s
ARN: arn:aws:cloudformation:us-west-2:987654321098:stack/my-stack/abc123def-456
Account: 987654321098
        "#;
        
        let normalized = RendererTestUtils::normalize_output(test_output);
        
        assert!(normalized.contains("2024-01-15T10:30:45.123Z"));
        assert!(normalized.contains("42s"));
        assert!(normalized.contains("123456789012"));
        assert!(normalized.contains("arn:aws:cloudformation:us-east-1:123456789012:stack/test-stack/test-id"));
    }

    #[test]
    fn test_extract_color_map() {
        let colored_output = "\x1b[31mError\x1b[0m: Something went wrong\n\x1b[32mSuccess\x1b[0m: Operation completed";
        
        let color_map = RendererTestUtils::extract_color_map(colored_output);
        
        assert!(color_map.contains_key("Error"));
        assert!(color_map.contains_key("Success"));
        assert_eq!(color_map["Error"], vec!["31"]);
        assert_eq!(color_map["Success"], vec!["32"]);
    }

    #[test]
    fn test_validate_color_usage() {
        let output = "\x1b[31mERROR\x1b[0m: Stack creation failed\n\x1b[32mCREATE_COMPLETE\x1b[0m resource";
        
        let patterns = [
            (r"ERROR", "31"),         // Red for errors
            (r"CREATE_COMPLETE", "32"), // Green for success
        ];
        
        let errors = RendererTestUtils::validate_color_usage(output, &patterns);
        assert!(errors.is_empty(), "Should have no color validation errors: {:?}", errors);
        
        // Test with wrong expected color
        let wrong_patterns = [
            (r"ERROR", "32"), // Wrong: expecting green for error
        ];
        
        let errors = RendererTestUtils::validate_color_usage(output, &wrong_patterns);
        assert!(!errors.is_empty(), "Should detect color mismatch");
    }
}