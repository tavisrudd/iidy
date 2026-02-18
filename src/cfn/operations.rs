//! CloudFormation operation types and utilities

use serde::{Deserialize, Serialize};

/// CloudFormation operation type
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CfnOperation {
    CreateStack,
    UpdateStack,
    DeleteStack,
    DescribeStack,
    CreateOrUpdate,
    CreateChangeset,
    ExecuteChangeset,
    EstimateCost,
    ListStacks,
    WatchStack,
    GetStackTemplate,
    DescribeStackDrift,
    TemplateApprovalRequest,
    TemplateApprovalReview,
}

impl CfnOperation {
    /// Convert from string (for backward compatibility)
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "create-stack" => Some(Self::CreateStack),
            "update-stack" => Some(Self::UpdateStack),
            "delete-stack" => Some(Self::DeleteStack),
            "describe-stack" => Some(Self::DescribeStack),
            "create-or-update" => Some(Self::CreateOrUpdate),
            "create-changeset" => Some(Self::CreateChangeset),
            "execute-changeset" => Some(Self::ExecuteChangeset),
            "estimate-cost" => Some(Self::EstimateCost),
            "list-stacks" => Some(Self::ListStacks),
            "watch-stack" => Some(Self::WatchStack),
            "get-stack-template" => Some(Self::GetStackTemplate),
            "describe-stack-drift" => Some(Self::DescribeStackDrift),
            "template-approval-request" => Some(Self::TemplateApprovalRequest),
            "template-approval-review" => Some(Self::TemplateApprovalReview),
            _ => None,
        }
    }

    /// Convert to string for display
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CreateStack => "create-stack",
            Self::UpdateStack => "update-stack",
            Self::DeleteStack => "delete-stack",
            Self::DescribeStack => "describe-stack",
            Self::CreateOrUpdate => "create-or-update",
            Self::CreateChangeset => "create-changeset",
            Self::ExecuteChangeset => "execute-changeset",
            Self::EstimateCost => "estimate-cost",
            Self::ListStacks => "list-stacks",
            Self::WatchStack => "watch-stack",
            Self::GetStackTemplate => "get-stack-template",
            Self::DescribeStackDrift => "describe-stack-drift",
            Self::TemplateApprovalRequest => "template-approval-request",
            Self::TemplateApprovalReview => "template-approval-review",
        }
    }

    /// Check if this operation is read-only (doesn't modify AWS resources)
    pub fn is_read_only(&self) -> bool {
        matches!(
            self,
            Self::DescribeStack
                | Self::EstimateCost
                | Self::ListStacks
                | Self::GetStackTemplate
                | Self::DescribeStackDrift // Note: TemplateApprovalRequest writes to S3, TemplateApprovalReview writes conditionally
        )
    }
}

impl std::fmt::Display for CfnOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
