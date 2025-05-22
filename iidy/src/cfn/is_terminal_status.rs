use aws_sdk_cloudformation::types::{ResourceStatus, StackStatus};

/// Return true if the [`ResourceStatus`] represents a terminal state.
///
/// Terminal states correspond to the COMPLETE/FAILED/SKIPPED sets from
/// the original Node.js implementation plus `REVIEW_IN_PROGRESS` for stack
/// statuses.
#[allow(dead_code)]
pub fn is_terminal_resource_status(status: &ResourceStatus) -> bool {
    matches!(
        status,
        ResourceStatus::CreateComplete
            | ResourceStatus::RollbackComplete
            | ResourceStatus::DeleteComplete
            | ResourceStatus::UpdateComplete
            | ResourceStatus::UpdateRollbackComplete
            | ResourceStatus::ImportComplete
            | ResourceStatus::ImportRollbackComplete
            | ResourceStatus::CreateFailed
            | ResourceStatus::DeleteFailed
            | ResourceStatus::RollbackFailed
            | ResourceStatus::UpdateRollbackFailed
            | ResourceStatus::ImportRollbackFailed
            | ResourceStatus::DeleteSkipped
    )
}

/// Return true if the [`StackStatus`] represents a terminal state.
#[allow(dead_code)]
pub fn is_terminal_stack_status(status: &StackStatus) -> bool {
    matches!(
        status,
        StackStatus::CreateComplete
            | StackStatus::RollbackComplete
            | StackStatus::DeleteComplete
            | StackStatus::UpdateComplete
            | StackStatus::UpdateRollbackComplete
            | StackStatus::ImportComplete
            | StackStatus::ImportRollbackComplete
            | StackStatus::CreateFailed
            | StackStatus::DeleteFailed
            | StackStatus::RollbackFailed
            | StackStatus::UpdateRollbackFailed
            | StackStatus::ImportRollbackFailed
            | StackStatus::ReviewInProgress
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_status_terminal() {
        assert!(is_terminal_resource_status(&ResourceStatus::CreateComplete));
        assert!(!is_terminal_resource_status(&ResourceStatus::CreateInProgress));
    }

    #[test]
    fn stack_status_terminal() {
        assert!(is_terminal_stack_status(&StackStatus::DeleteComplete));
        assert!(!is_terminal_stack_status(&StackStatus::UpdateInProgress));
    }
}
