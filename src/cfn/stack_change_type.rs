//! Stack change type definitions for create-or-update operations

use serde::{Deserialize, Serialize};

/// Type of stack change operation being performed
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StackChangeType {
    /// Creating a new stack
    Create,
    /// Updating an existing stack with changes detected
    UpdateWithChanges { stack_id: String },
    /// Attempting to update but no changes were detected
    UpdateNoChanges,
}

/// Result of attempting a stack update operation
#[derive(Clone, Debug)]
pub enum UpdateResult {
    /// No changes were detected, no update needed
    NoChanges,
    /// Update was initiated successfully, returns the stack ID
    StackId(String),
}