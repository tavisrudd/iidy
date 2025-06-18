//! CloudFormation status constants and utility functions
//!
//! This module contains the exact status arrays from the original iidy-js implementation
//! for consistent status checking and categorization across all output modes.

/// Stack and resource statuses that indicate an operation is in progress
pub const IN_PROGRESS: &[&str] = &[
    "CREATE_IN_PROGRESS",
    "REVIEW_IN_PROGRESS", 
    "ROLLBACK_IN_PROGRESS",
    "DELETE_IN_PROGRESS",
    "UPDATE_IN_PROGRESS",
    "UPDATE_COMPLETE_CLEANUP_IN_PROGRESS",
    "UPDATE_ROLLBACK_IN_PROGRESS",
    "UPDATE_ROLLBACK_COMPLETE_CLEANUP_IN_PROGRESS",
    "IMPORT_IN_PROGRESS",
    "IMPORT_ROLLBACK_IN_PROGRESS",
];

/// Stack and resource statuses that indicate successful completion
pub const COMPLETE: &[&str] = &[
    "CREATE_COMPLETE",
    "ROLLBACK_COMPLETE",
    "DELETE_COMPLETE",
    "UPDATE_COMPLETE",
    "UPDATE_ROLLBACK_COMPLETE",
    "IMPORT_COMPLETE",
    "IMPORT_ROLLBACK_COMPLETE",
];

/// Stack and resource statuses that indicate failure
pub const FAILED: &[&str] = &[
    "CREATE_FAILED",
    "DELETE_FAILED",
    "ROLLBACK_FAILED",
    "UPDATE_ROLLBACK_FAILED",
    "IMPORT_ROLLBACK_FAILED",
];

/// Stack and resource statuses that indicate skipped operations
pub const SKIPPED: &[&str] = &[
    "DELETE_SKIPPED"
];

/// All statuses that indicate a terminal state (operation complete, no further events expected)
pub const TERMINAL: &[&str] = &[
    // All COMPLETE statuses
    "CREATE_COMPLETE",
    "ROLLBACK_COMPLETE", 
    "DELETE_COMPLETE",
    "UPDATE_COMPLETE",
    "UPDATE_ROLLBACK_COMPLETE",
    "IMPORT_COMPLETE",
    "IMPORT_ROLLBACK_COMPLETE",
    // All FAILED statuses
    "CREATE_FAILED",
    "DELETE_FAILED",
    "ROLLBACK_FAILED",
    "UPDATE_ROLLBACK_FAILED",
    "IMPORT_ROLLBACK_FAILED",
    // SKIPPED statuses
    "DELETE_SKIPPED",
    // Special case
    "REVIEW_IN_PROGRESS", // Terminal for change sets
];

/// Status categorization for determining colors and icons
#[derive(Debug, Clone, PartialEq)]
pub enum StatusCategory {
    InProgress,
    Complete,
    Failed,
    Skipped,
    Terminal,
    Unknown,
}

/// Determine the category of a CloudFormation status
pub fn categorize_status(status: &str) -> StatusCategory {
    if IN_PROGRESS.contains(&status) {
        StatusCategory::InProgress
    } else if COMPLETE.contains(&status) {
        StatusCategory::Complete
    } else if FAILED.contains(&status) {
        StatusCategory::Failed
    } else if SKIPPED.contains(&status) {
        StatusCategory::Skipped
    } else if TERMINAL.contains(&status) {
        StatusCategory::Terminal
    } else {
        StatusCategory::Unknown
    }
}

/// Check if a status indicates an operation is still in progress
pub fn is_in_progress(status: &str) -> bool {
    IN_PROGRESS.contains(&status)
}

/// Check if a status indicates successful completion
pub fn is_complete(status: &str) -> bool {
    COMPLETE.contains(&status)
}

/// Check if a status indicates failure
pub fn is_failed(status: &str) -> bool {
    FAILED.contains(&status)
}

/// Check if a status indicates a skipped operation
pub fn is_skipped(status: &str) -> bool {
    SKIPPED.contains(&status)
}

/// Check if a status indicates a terminal state (no more events expected)
pub fn is_terminal(status: &str) -> bool {
    TERMINAL.contains(&status)
}

/// Get an appropriate emoji icon for a status category
pub fn status_icon(status: &str) -> &'static str {
    match categorize_status(status) {
        StatusCategory::InProgress => "🔄",
        StatusCategory::Complete => "✅",
        StatusCategory::Failed => "❌",
        StatusCategory::Skipped => "⏭️",
        StatusCategory::Terminal => "🏁",
        StatusCategory::Unknown => "❓",
    }
}

/// Get a human-readable description of what a status means
pub fn status_description(status: &str) -> &'static str {
    match status {
        "CREATE_IN_PROGRESS" => "Creating resource",
        "CREATE_COMPLETE" => "Resource created successfully", 
        "CREATE_FAILED" => "Resource creation failed",
        "DELETE_IN_PROGRESS" => "Deleting resource",
        "DELETE_COMPLETE" => "Resource deleted successfully",
        "DELETE_FAILED" => "Resource deletion failed",
        "DELETE_SKIPPED" => "Resource deletion skipped",
        "UPDATE_IN_PROGRESS" => "Updating resource",
        "UPDATE_COMPLETE" => "Resource updated successfully",
        "UPDATE_FAILED" => "Resource update failed",
        "ROLLBACK_IN_PROGRESS" => "Rolling back changes",
        "ROLLBACK_COMPLETE" => "Rollback completed successfully",
        "ROLLBACK_FAILED" => "Rollback failed",
        "REVIEW_IN_PROGRESS" => "Change set awaiting review",
        "IMPORT_IN_PROGRESS" => "Importing resource",
        "IMPORT_COMPLETE" => "Resource imported successfully",
        "IMPORT_ROLLBACK_IN_PROGRESS" => "Rolling back import",
        "IMPORT_ROLLBACK_COMPLETE" => "Import rollback completed",
        "IMPORT_ROLLBACK_FAILED" => "Import rollback failed",
        "UPDATE_ROLLBACK_IN_PROGRESS" => "Rolling back update",
        "UPDATE_ROLLBACK_COMPLETE" => "Update rollback completed",
        "UPDATE_ROLLBACK_FAILED" => "Update rollback failed",
        "UPDATE_COMPLETE_CLEANUP_IN_PROGRESS" => "Cleaning up after update",
        "UPDATE_ROLLBACK_COMPLETE_CLEANUP_IN_PROGRESS" => "Cleaning up after rollback",
        _ => "Unknown status",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_categorization() {
        // Test IN_PROGRESS statuses
        assert_eq!(categorize_status("CREATE_IN_PROGRESS"), StatusCategory::InProgress);
        assert_eq!(categorize_status("UPDATE_IN_PROGRESS"), StatusCategory::InProgress);
        assert!(is_in_progress("CREATE_IN_PROGRESS"));
        assert!(!is_terminal("CREATE_IN_PROGRESS"));

        // Test COMPLETE statuses
        assert_eq!(categorize_status("CREATE_COMPLETE"), StatusCategory::Complete);
        assert_eq!(categorize_status("UPDATE_COMPLETE"), StatusCategory::Complete);
        assert!(is_complete("CREATE_COMPLETE"));
        assert!(is_terminal("CREATE_COMPLETE"));

        // Test FAILED statuses
        assert_eq!(categorize_status("CREATE_FAILED"), StatusCategory::Failed);
        assert_eq!(categorize_status("UPDATE_ROLLBACK_FAILED"), StatusCategory::Failed);
        assert!(is_failed("CREATE_FAILED"));
        assert!(is_terminal("CREATE_FAILED"));

        // Test SKIPPED statuses
        assert_eq!(categorize_status("DELETE_SKIPPED"), StatusCategory::Skipped);
        assert!(is_skipped("DELETE_SKIPPED"));
        assert!(is_terminal("DELETE_SKIPPED"));

        // Test unknown status
        assert_eq!(categorize_status("UNKNOWN_STATUS"), StatusCategory::Unknown);
        assert!(!is_in_progress("UNKNOWN_STATUS"));
        assert!(!is_complete("UNKNOWN_STATUS"));
        assert!(!is_failed("UNKNOWN_STATUS"));
        assert!(!is_terminal("UNKNOWN_STATUS"));
    }

    #[test]
    fn test_terminal_status_detection() {
        // All COMPLETE statuses should be terminal
        for status in COMPLETE {
            assert!(is_terminal(status), "{} should be terminal", status);
        }

        // All FAILED statuses should be terminal
        for status in FAILED {
            assert!(is_terminal(status), "{} should be terminal", status);
        }

        // All SKIPPED statuses should be terminal
        for status in SKIPPED {
            assert!(is_terminal(status), "{} should be terminal", status);
        }

        // IN_PROGRESS statuses should not be terminal (except REVIEW_IN_PROGRESS)
        for status in IN_PROGRESS {
            if *status == "REVIEW_IN_PROGRESS" {
                assert!(is_terminal(status), "REVIEW_IN_PROGRESS should be terminal");
            } else {
                assert!(!is_terminal(status), "{} should not be terminal", status);
            }
        }
    }

    #[test]
    fn test_status_icons() {
        assert_eq!(status_icon("CREATE_IN_PROGRESS"), "🔄");
        assert_eq!(status_icon("CREATE_COMPLETE"), "✅");
        assert_eq!(status_icon("CREATE_FAILED"), "❌");
        assert_eq!(status_icon("DELETE_SKIPPED"), "⏭️");
        assert_eq!(status_icon("UNKNOWN_STATUS"), "❓");
    }

    #[test]
    fn test_status_descriptions() {
        assert_eq!(status_description("CREATE_COMPLETE"), "Resource created successfully");
        assert_eq!(status_description("DELETE_FAILED"), "Resource deletion failed");
        assert_eq!(status_description("REVIEW_IN_PROGRESS"), "Change set awaiting review");
        assert_eq!(status_description("UNKNOWN_STATUS"), "Unknown status");
    }

    #[test]
    fn test_status_arrays_no_duplicates() {
        // Verify no status appears in multiple primary categories
        let mut all_statuses = Vec::new();
        all_statuses.extend(IN_PROGRESS.iter());
        all_statuses.extend(COMPLETE.iter());
        all_statuses.extend(FAILED.iter());
        all_statuses.extend(SKIPPED.iter());

        let mut seen = std::collections::HashSet::new();
        for status in all_statuses {
            assert!(seen.insert(status), "Duplicate status found: {}", status);
        }
    }

    #[test]
    fn test_terminal_array_completeness() {
        // TERMINAL should contain all COMPLETE, FAILED, SKIPPED + REVIEW_IN_PROGRESS
        let mut expected_terminal = Vec::new();
        expected_terminal.extend(COMPLETE.iter());
        expected_terminal.extend(FAILED.iter());
        expected_terminal.extend(SKIPPED.iter());
        expected_terminal.push(&"REVIEW_IN_PROGRESS");

        for status in expected_terminal {
            assert!(TERMINAL.contains(status), "TERMINAL missing status: {}", status);
        }
    }
}