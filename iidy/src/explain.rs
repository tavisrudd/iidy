//! Error code explanation functionality
//!
//! This module provides the implementation for the `explain` command,
//! which shows detailed information about error codes.

use crate::yaml::error_ids::ErrorId;

/// Handle the explain command - show detailed explanations for error codes
/// 
/// Takes a list of error codes (like "IY2001", "IY4002") and prints
/// detailed explanations for each one.
/// 
/// # Arguments
/// * `codes` - Vector of error code strings to explain
pub fn handle_explain_command(codes: Vec<String>) {
    if codes.is_empty() {
        eprintln!("Please provide one or more error codes to explain (e.g., IY2001)");
        return;
    }
    
    for code in codes {
        // Try to parse the error code
        if let Some(error_id) = ErrorId::from_code(&code) {
            println!("{}", error_id.explain());
            println!();
        } else {
            eprintln!("Unknown error code: {}", code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_handle_explain_command_empty() {
        // This test just ensures the function doesn't panic with empty input
        // The actual output goes to stderr/stdout so we can't easily test it
        handle_explain_command(vec![]);
    }
    
    #[test]
    fn test_handle_explain_command_valid_code() {
        // Test with a valid error code
        handle_explain_command(vec!["IY2001".to_string()]);
    }
    
    #[test]
    fn test_handle_explain_command_invalid_code() {
        // Test with an invalid error code
        handle_explain_command(vec!["INVALID".to_string()]);
    }
    
    #[test]
    fn test_handle_explain_command_multiple_codes() {
        // Test with multiple codes
        handle_explain_command(vec!["IY2001".to_string(), "IY4002".to_string()]);
    }
}