# Template Approval Commands: Design Specification

## Overview

This document provides a complete specification and design for implementing the `template-approval` commands (`request` and `review`) in the iidy Rust rewrite, following the established output sequencing architecture and data-driven patterns.

## Executive Summary

The template approval system provides a workflow for reviewing and approving CloudFormation template changes before deployment. It consists of two commands:

1. **`template-approval request`**: Uploads a template for approval to S3 with a `.pending` suffix
2. **`template-approval review`**: Reviews pending templates, shows diffs, and approves/rejects changes

This system enables organizations to implement approval workflows for infrastructure changes while maintaining change audit trails.

## Architecture Alignment

Following the **ADR-2025-07-06-output-sequencing-architecture.md**, this implementation will:

- Use the **`run_command_handler!` macro pattern** for clean AWS context management
- Follow **data-driven output architecture** with appropriate `OutputData` variants
- Support **Interactive, Plain, and JSON output modes**
- Implement **proper error handling** with the `??` pattern
- Use **tokio::spawn** for concurrent S3 operations when beneficial
- Leverage **existing infrastructure** (template loading, S3 client creation, confirmation prompts)
- Both operations are **NOT read-only** (they modify S3), so will use NTP time sync

## Detailed Command Specifications

### 1. `template-approval request <argsfile>`

#### Purpose
Upload a CloudFormation template for approval review, generating a versioned S3 location based on template content hash.

#### CLI Arguments
```rust
#[derive(Args, Debug, Clone)]
pub struct ApprovalRequestArgs {
    /// Path to the stack arguments YAML file
    pub argsfile: String,
    
    /// Enable template linting (default: true)
    #[arg(long = "lint-template", default_value_t = true)]
    pub lint_template: bool,
    
    /// Use parameters to improve linting accuracy
    #[arg(long = "lint-using-parameters")]
    pub lint_using_parameters: bool,
}
```

#### Workflow
1. **Load Stack Args**: Parse `argsfile` using existing infrastructure:
   - Use existing stack args loading pattern
   - Validate `ApprovedTemplateLocation` (required S3 URL prefix)
   - Validate `Template` (template file path)
   - Load `Parameters` (if `lint_using_parameters` is true)

2. **Generate Versioned Location**: 
   - Load template using `template_loader::load_cfn_template()` with:
     - Template location from stack args
     - Argsfile path for relative resolution
     - Environment for preprocessing
     - `TEMPLATE_MAX_BYTES` constant (for consistency)
     - S3 client from `context.create_s3_client()` for S3 templates
   - This ensures all YAML preprocessing, imports, and transformations are applied
   - Calculate MD5 hash using `md5::compute()` on the processed template body
   - Generate S3 key: `{prefix}/{hash}{extension}`

3. **Check Existing Approval**:
   - Check if versioned template already exists in S3
   - If exists: Display "already approved" message
   - If not: Proceed to upload

4. **Template Validation** (if enabled):
   - Lint template using CloudFormation validation
   - Include parameters if `lint_using_parameters` is true
   - Fail if validation errors exist

5. **Upload Pending Template**:
   - Upload to S3 with `.pending` suffix
   - Set appropriate ACLs (`bucket-owner-full-control`)
   - Display success message with review command

#### Expected Output Sections
- `command_metadata` (environment, region, arguments)
- `template_validation` (if linting enabled)
- `approval_request_result` (upload status and next steps)

### 2. `template-approval review <url>`

#### Purpose
Review a pending template approval request, show differences, and approve/reject changes.

#### CLI Arguments
```rust
#[derive(Args, Debug, Clone)]
pub struct ApprovalReviewArgs {
    /// S3 URL to the pending template (s3://bucket/path.pending)
    pub url: String,
    
    /// Number of lines of diff context to show
    #[arg(long, default_value_t = 100)]
    pub context: u32,
}
```

#### Workflow
1. **Parse S3 URL**: Extract bucket and key from provided URL

2. **Check Approval Status**:
   - Check if non-pending version already exists
   - If exists: Display "already approved" message and exit

3. **Fetch Templates**:
   - Download pending template from S3
   - Download latest approved template (or empty if first approval)
   - Handle missing "latest" gracefully

4. **Show Diff**:
   - Generate unified diff between templates
   - Display using git-style colored diff output
   - Respect `context` parameter for diff context lines

5. **Request Confirmation**:
   - Use existing `ConfirmationRequest` infrastructure
   - Prompt user: "Would you like to approve these changes?"
   - Handle user response through output manager

6. **Approval Process** (if confirmed):
   - Upload approved template (remove `.pending` suffix)
   - Update `latest` symlink/copy
   - Delete pending template
   - Display success confirmation

#### Expected Output Sections
- `command_metadata` (environment, region, arguments)
- `approval_status` (current approval state)
- `template_diff` (visual diff display)
- `confirmation` (approval prompt)
- `approval_result` (final outcome)

## Data Structures

### New OutputData Variants

```rust
/// Template approval request result
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApprovalRequestResult {
    pub template_location: String,
    pub pending_location: String,
    pub already_approved: bool,
    pub next_steps: Vec<String>,
}

/// Template validation results
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateValidation {
    pub enabled: bool,
    pub using_parameters: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Template approval status
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApprovalStatus {
    pub pending_exists: bool,
    pub already_approved: bool,
    pub pending_location: String,
    pub approved_location: Option<String>,
}

/// Template diff display
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateDiff {
    pub old_template: String,
    pub new_template: String,
    pub diff_output: String,
    pub context_lines: u32,
    pub has_changes: bool,
}

/// Approval result
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApprovalResult {
    pub approved: bool,
    pub approved_location: Option<String>,
    pub latest_location: Option<String>,
    pub cleanup_completed: bool,
}

// Add to OutputData enum:
pub enum OutputData {
    // ... existing variants
    ApprovalRequestResult(ApprovalRequestResult),
    TemplateValidation(TemplateValidation),
    ApprovalStatus(ApprovalStatus), 
    TemplateDiff(TemplateDiff),
    ApprovalResult(ApprovalResult),
}
```

## Implementation Architecture

### Module Structure
```
src/cfn/
├── template_approval_request.rs  # template-approval request implementation
├── template_approval_review.rs   # template-approval review implementation
├── template_hash.rs             # Template hashing and versioning (already implemented)
└── (uses existing infrastructure for S3, template loading, etc.)
```

### Key Dependencies
- `load_stack_args()` - Existing function for loading stack args YAML files
- `template_loader::load_cfn_template()` - Existing template loading with preprocessing
- `CfnContext::create_s3_client()` - Existing S3 client creation
- `similar` crate - For cross-platform diff generation
- `ConfirmationRequest` - Existing confirmation prompt infrastructure

### Core Implementation Pattern

Following the `run_command_handler!` pattern:

```rust
// src/cfn/approval/request.rs
pub async fn template_approval_request(cli: &Cli, args: &ApprovalRequestArgs) -> Result<i32> {
    run_command_handler!(template_approval_request_impl, cli, args)
}

async fn template_approval_request_impl(
    output_manager: &mut DynamicOutputManager,
    context: &crate::cfn::CfnContext,
    _cli: &Cli,
    args: &ApprovalRequestArgs,
    _opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    // Load stack args
    let stack_args = load_stack_args(
        &args.argsfile,
        &cli.global_opts.environment,
        &CfnOperation::TemplateApprovalRequest,
        &AwsSettings::from_normalized_opts(opts),
    ).await?;
    
    // Validate required fields
    if stack_args.approved_template_location.is_none() {
        anyhow::bail!("ApprovedTemplateLocation is required in stack-args.yaml");
    }
    if stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }
    
    // Load template using standard loader (includes all preprocessing)
    let template_result = load_cfn_template(
        stack_args.template.as_ref(),
        &args.argsfile,
        Some(&cli.global_opts.environment),
        TEMPLATE_MAX_BYTES,
        Some(&context.create_s3_client()),
    ).await?;
    
    let template_body = template_result.template_body
        .ok_or_else(|| anyhow::anyhow!("Failed to load template body"))?;
    
    // Generate versioned location
    let (bucket, key) = generate_versioned_location(
        &stack_args.approved_template_location.unwrap(),
        &template_body,
        stack_args.template.as_ref().unwrap(),
    )?;
    
    // Process approval workflow
    let result = process_approval_request(&context, &stack_args, &template_result).await?;
    output_manager.render(OutputData::ApprovalRequestResult(result)).await?;
    
    Ok(if result.already_approved { 0 } else { 0 })
}
```

### S3 Operations Design

```rust
// S3 operations will use the existing CfnContext pattern
impl CfnContext {
    // Already provides: create_s3_client() -> S3Client
}

// Helper functions in the approval modules
async fn check_template_exists(s3_client: &S3Client, bucket: &str, key: &str) -> Result<bool> {
    match s3_client.head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await {
        Ok(_) => Ok(true),
        Err(e) if is_not_found(&e) => Ok(false),
        Err(e) => Err(e.into()),
    }
}

async fn upload_pending_template(s3_client: &S3Client, bucket: &str, key: &str, content: &str) -> Result<()> {
    s3_client.put_object()
        .bucket(bucket)
        .key(format!("{}.pending", key))
        .body(content.into())
        .acl(aws_sdk_s3::types::ObjectCannedAcl::BucketOwnerFullControl)
        .send()
        .await?;
    Ok(())
}

// Similar patterns for other S3 operations...
```

### Template Hashing

```rust
// src/cfn/template_hash.rs (already implemented)
pub fn calculate_template_hash(template_content: &str) -> String {
    let digest = md5::compute(template_content.as_bytes());
    format!("{:x}", digest)
}

pub fn generate_versioned_location(
    base_location: &str, 
    template_content: &str, 
    template_path: &str
) -> Result<(String, String)> {
    // Parse S3 URL, calculate hash, return (bucket, key)
    // Already implemented with proper error handling
}
```

### Diff Generation

```rust
// Use the `similar` crate for cross-platform colored diffs
// This avoids the iidy-js approach of shelling out to git diff
pub fn generate_template_diff(old: &str, new: &str, context: u32) -> Result<String> {
    use similar::{ChangeTag, TextDiff};
    use owo_colors::OwoColorize;
    
    let diff = TextDiff::from_lines(old, new);
    let mut output = String::new();
    
    // Generate git-style unified diff with proper coloring
    for (idx, group) in diff.grouped_ops(context as usize).iter().enumerate() {
        if idx > 0 {
            output.push_str(&"---".dimmed().to_string());
            output.push('\n');
        }
        
        for op in group {
            for change in diff.iter_inline_changes(op) {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                
                output.push_str(&match change.tag() {
                    ChangeTag::Delete => format!("{}", sign.red()),
                    ChangeTag::Insert => format!("{}", sign.green()),
                    ChangeTag::Equal => sign.to_string(),
                });
                
                for (_, value) in change.iter_strings_lossy() {
                    output.push_str(&match change.tag() {
                        ChangeTag::Delete => value.red().to_string(),
                        ChangeTag::Insert => value.green().to_string(),
                        ChangeTag::Equal => value.to_string(),
                    });
                }
                
                if change.missing_newline() {
                    output.push('\n');
                }
            }
        }
    }
    
    Ok(output)
}
```

## Renderer Integration

### Interactive Renderer Extensions

```rust
// src/output/renderers/interactive.rs - new methods
impl InteractiveRenderer {
    async fn render_approval_request_result(&mut self, data: &ApprovalRequestResult) -> Result<()> {
        if data.already_approved {
            println!("{}", "👍 Your template has already been approved".color(self.theme.success));
        } else {
            self.print_section_heading_with_newline("Template Approval Request");
            println!("Successfully uploaded template to: {}", data.pending_location.color(self.theme.muted));
            println!();
            println!("Approve template with:");
            println!("  {}", format!("iidy template-approval review {}", data.pending_location).color(self.theme.primary));
        }
        Ok(())
    }
    
    async fn render_template_diff(&mut self, data: &TemplateDiff) -> Result<()> {
        if !data.has_changes {
            println!("{}", "Templates are identical".color(self.theme.success));
        } else {
            self.print_section_heading_with_newline("Template Changes");
            // Display colorized diff output
            print!("{}", data.diff_output);
        }
        Ok(())
    }
    
    async fn render_approval_result(&mut self, data: &ApprovalResult) -> Result<()> {
        if data.approved {
            println!();
            println!("{}", "Template has been successfully approved!".color(self.theme.success));
        } else {
            println!("{}", "Approval cancelled".color(self.theme.warning));
        }
        Ok(())
    }
}
```

### JSON Renderer Extensions

```rust
// src/output/renderers/json.rs - new methods  
impl JsonRenderer {
    async fn render_approval_request_result(&mut self, data: &ApprovalRequestResult) -> Result<()> {
        self.output_json("approval_request_result", data)
    }
    
    async fn render_template_diff(&mut self, data: &TemplateDiff) -> Result<()> {
        self.output_json("template_diff", data)  
    }
    
    async fn render_approval_result(&mut self, data: &ApprovalResult) -> Result<()> {
        self.output_json("approval_result", data)
    }
}
```

## Section Sequencing

### Template Approval Request Sections
```rust
CfnOperation::TemplateApprovalRequest => vec![
    "command_metadata",
    "template_validation", 
    "approval_request_result"
],
```

### Template Approval Review Sections  
```rust
CfnOperation::TemplateApprovalReview => vec![
    "command_metadata",
    "approval_status",
    "template_diff",
    "confirmation",
    "approval_result"
],
```

## Error Handling

### AWS-Specific Errors
- **S3 Access Denied**: Clear message about bucket permissions
- **Template Not Found**: Helpful guidance on correct URLs
- **Invalid S3 URL**: Parse and validate URL format

### Template Validation Errors
- **Linting Failures**: Display all validation errors with context
- **Missing Required Fields**: Guide user to required stack args
- **Template Load Errors**: YAML parsing and import resolution errors

### User Experience Errors
- **Approval Already Exists**: Informative success message, not error
- **User Cancellation**: Clean exit with appropriate messaging

## Security Considerations

### S3 Permissions
- Templates uploaded with `bucket-owner-full-control` ACL
- Support for cross-account bucket scenarios
- Proper error handling for permission failures

### Template Content
- No sensitive data exposure in error messages
- Secure temporary file handling for diff operations
- Proper cleanup of downloaded template content

## Testing Strategy

### Unit Tests
- Template hashing consistency
- S3 URL parsing and validation
- Diff generation accuracy
- Error condition handling

### Integration Tests
- End-to-end approval workflow
- Multi-user approval scenarios
- S3 permission edge cases
- Template validation integration

### Snapshot Tests
- Interactive output formatting
- JSON output structure
- Error message consistency

## Migration Strategy

### Phase 1: Core Implementation
1. Implement basic `request` and `review` commands
2. Add S3 operations and template hashing
3. Create basic output data structures
4. Implement interactive renderer support

### Phase 2: Advanced Features
1. Add template validation/linting
2. Implement diff generation with colors
3. Add comprehensive error handling
4. Complete JSON renderer support

### Phase 3: Polish and Testing
1. Add comprehensive test coverage
2. Performance optimization
3. Documentation and examples
4. User acceptance testing

## Future Enhancements

### Potential Extensions
1. **Approval Workflows**: Multi-approver support
2. **Audit Trails**: Enhanced logging and tracking
3. **Integration**: Webhook notifications
4. **Advanced Diff**: Semantic CloudFormation diff
5. **Batch Operations**: Multiple template approvals

### Configuration Options
1. **Approval Policies**: Configurable approval requirements
2. **S3 Settings**: Custom ACLs and encryption
3. **Validation Rules**: Custom linting configurations

## Conclusion

This specification provides a comprehensive blueprint for implementing template approval commands that:

- **Follow established patterns** from the existing codebase
- **Support all output modes** (Interactive, Plain, JSON)
- **Provide professional UX** with proper error handling
- **Enable audit workflows** for infrastructure changes
- **Maintain security** with appropriate access controls

The implementation will enhance iidy's enterprise capabilities while maintaining consistency with the established architecture and user experience patterns.

## Implementation Notes (Updated)

### Key Discoveries
1. **CLI Structure**: The `ApprovalCommands` enum and argument structures already exist in `src/cli.rs`
2. **S3 Client Pattern**: Use `CfnContext::create_s3_client()` for consistency
3. **Stack Args Loading**: Use existing `load_stack_args()` function
4. **Template Loading**: Use `template_loader::load_cfn_template()` for preprocessing support
5. **Confirmation Prompts**: Use existing `ConfirmationRequest` infrastructure

### Architecture Decisions
1. **Module Structure**: Keep modules flat in `src/cfn/` rather than creating subdirectory
2. **S3 Operations**: Use helper functions with the context's S3 client rather than separate struct
3. **Diff Generation**: Use `similar` crate for cross-platform support (avoid shelling out)
4. **Operation Type**: Both commands are NOT read-only (they modify S3)

### Dependencies Required
- `similar` crate for diff generation
- `md5` crate for template hashing (already added)

### Integration Points
1. **Stack Args**: `load_stack_args()` function - loads YAML with `ApprovedTemplateLocation` field
2. **Templates**: `template_loader::load_cfn_template()` - MUST use this for all preprocessing
   - Pass template location from `stack_args.template`
   - Pass argsfile path for relative resolution
   - Pass environment for variable substitution
   - Pass `TEMPLATE_MAX_BYTES` constant
   - Pass S3 client from context for S3 templates
3. **S3 Client**: `context.create_s3_client()` - use for all S3 operations
4. **Confirmations**: `ConfirmationRequest` output type - existing prompt infrastructure
5. **Rendering**: Data-driven output system - all output through `OutputData` variants

### Critical Implementation Note
**Template Loading**: The approval system MUST use `load_cfn_template()` to ensure:
- All YAML preprocessing is applied (!include, !import, etc.)
- Variables are resolved
- Handlebars templates are processed
- The hash is calculated on the FINAL processed template, not the raw source

This ensures the approved template hash matches what will actually be deployed.