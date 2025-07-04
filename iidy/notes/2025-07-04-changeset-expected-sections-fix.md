# Changeset Expected Sections Fix

**Date:** 2025-07-04  
**Purpose:** Fix incorrect expected_sections configuration for CreateOrUpdate with --changeset flag

## Problem Statement

When `create-or-update` is called with the `--changeset` flag, the interactive renderer's `expected_sections` configuration is incorrect. This causes output sections to be displayed out of order or missing entirely.

### Current Behavior

The output shows:
```
Stack Change Details:
? Do you want to execute this changeset now? (y/N) y
Stack Details:
⠼ Loading stack details...
Fri Jul 04 2025 21:33:47 Executing changeset 'swift-eagle' for stack: iidy-demo-hello-world
⠋ Loading stack details...
Fri Jul 04 2025 21:33:47 Changeset execution initiated
Fri Jul 04 2025 21:33:47 Watching stack operation progress...
⠦ Loading stack details...
Fri Jul 04 2025 21:33:53 Stack operation completed successfully

SUCCESS: (6s)
Changeset execution completed
```

### Expected Behavior (from iidy-js)

The output should match the iidy-js pattern:
```
Command Metadata:
 CFN Operation:           CREATE_CHANGESET
 iidy Environment:        development
 Region:                  us-east-1
 CLI Arguments:           region=null, profile=null, argsfile=example-stacks/hello-world/stack-args.yaml
 IAM Service Role:        None
 Current IAM Principal:   arn:aws:iam::903405759226:user/tavis-cli
 iidy Version:            1.12.0

AWS Console URL for full changeset review: https://us-east-1.console.aws.amazon.com/cloudformation/home?region=us-east-1#/changeset/detail?stackId=...

Pending Changesets:
 Fri Jul 04 2025 14:35:22 snobbish-tongue CREATE_COMPLETE
  Add     TestWaitHandle                 AWS::CloudFormation::WaitConditionHandle

Your new stack is now in REVIEW_IN_PROGRESS state. To create the resources run the following
  iidy --region us-east-1 exec-changeset --stack-name iidy-demo-hello-world example-stacks/hello-world/stack-args.yaml snobbish-tongue

Command Summary:          Success 👍

? Do you want to execute this changeset now? Yes

Command Metadata:
 CFN Operation:           EXECUTE_CHANGESET
 iidy Environment:        development
 Region:                  us-east-1
 CLI Arguments:           region=null, profile=null, argsfile=example-stacks/hello-world/stack-args.yaml
 IAM Service Role:        None
 Current IAM Principal:   arn:aws:iam::903405759226:user/tavis-cli
 iidy Version:            1.12.0

Stack Details:
 Name:                    iidy-demo-hello-world
 Status                   CREATE_IN_PROGRESS
 Capabilities:            None
 Service Role:            None
 Tags:                    environment=development, lifetime=short, project=iidy-demo
 Parameters:              Name=world
 DisableRollback:         false
 TerminationProtection:   false
 Creation Time:           Fri Jul 04 2025 14:35:22
 Last Update Time:        Fri Jul 04 2025 14:35:34
 NotificationARNs:        None
 ARN:                     arn:aws:cloudformation:us-east-1:903405759226:stack/iidy-demo-hello-world/ca7372e0-591e-11f0-9a9b-12a34c439513
 Console URL:             https://us-east-1.console.aws.amazon.com/cloudformation/home?region=us-east-1#/stack/detail?stackId=...

Previous Stack Events (max 10):
 Fri Jul 04 2025 14:35:22 REVIEW_IN_PROGRESS AWS::CloudFormation::Stack               iidy-demo-hello-world

Live Stack Events (2s poll):
 Fri Jul 04 2025 14:35:34 CREATE_IN_PROGRESS AWS::CloudFormation::Stack               iidy-demo-hello-world
 Fri Jul 04 2025 14:35:36 CREATE_IN_PROGRESS AWS::CloudFormation::WaitConditionHandle TestWaitHandle
 Fri Jul 04 2025 14:35:36 CREATE_IN_PROGRESS AWS::CloudFormation::WaitConditionHandle TestWaitHandle
 Fri Jul 04 2025 14:35:37 CREATE_COMPLETE    AWS::CloudFormation::WaitConditionHandle TestWaitHandle (1s)
 Fri Jul 04 2025 14:35:37 CREATE_COMPLETE    AWS::CloudFormation::Stack               iidy-demo-hello-world (4s)
 6 seconds elapsed total.

Stack Resources:
 TestWaitHandle AWS::CloudFormation::WaitConditionHandle
https://cloudformation-waitcondition-us-east-1.s3.amazonaws.com/...

Stack Outputs: None

Current Stack Status:     CREATE_COMPLETE
Command Summary:          Success 👍
```

## Root Cause Analysis

### 1. Static Expected Sections

The `CreateOrUpdate` operation has a static `expected_sections` configuration in `interactive.rs:499`:
```rust
CfnOperation::CreateOrUpdate => vec!["command_metadata", "stack_change_details", "stack_definition", "live_stack_events", "stack_contents"],
```

This doesn't account for the different behavior when `--changeset` is used.

### 2. CreateOrUpdate with --changeset Flow

When `CreateOrUpdate` is called with `--changeset`, it follows a different execution path in `create_or_update.rs`:

```rust
if args.changeset {
    if stack_exists {
        return update_stack_with_changeset_data(&context, args, &final_stack_args, &mut output_manager).await;
    } else {
        return create_stack_with_changeset_data(&context, args, &final_stack_args, &mut output_manager, &global_opts, &opts).await;
    }
}
```

These helper functions effectively perform two separate operations:
1. **Create Changeset** (similar to `CreateChangeset` operation)
2. **Execute Changeset** (similar to `ExecuteChangeset` operation after confirmation)

### 3. The Problem

The issue is that `CreateOrUpdate` with `--changeset` needs different expected sections for each phase:

**Phase 1 - Changeset Creation:**
- Should have sections: `["command_metadata", "changeset_result", "confirmation"]`
- Should display changeset details and prompt for confirmation
- The confirmation section should ask "Do you want to execute this changeset now?"

**Phase 2 - Changeset Execution (after confirmation):**
- Should have sections: `["command_metadata", "stack_definition", "stack_events", "live_stack_events", "stack_contents"]`
- Should display full stack monitoring like a regular update operation
- This phase only happens if the user responds "yes" to the confirmation

Currently, the renderer is configured with the wrong sections for the changeset flow, causing:
- Missing changeset result display
- Missing stack definition after execution
- Missing previous stack events
- Incorrect section ordering

## Design Considerations

### Option 1: Dynamic Section Configuration (Not Feasible)

We could try to make `expected_sections` dynamic based on the `--changeset` flag, but this would require:
- Access to the CLI args in the renderer setup
- Complex logic to handle the two-phase nature of changeset operations
- Significant refactoring of the renderer initialization

### Option 2: Dynamic Expected Sections Based on Phase (Recommended)

Since `CreateOrUpdate` with `--changeset` has two distinct phases with a confirmation prompt in between, we need to handle the section transitions dynamically:

1. Start with Phase 1 sections: `["command_metadata", "changeset_result", "confirmation"]`
2. After user confirms, transition to Phase 2 sections: `["command_metadata", "stack_definition", "stack_events", "live_stack_events", "stack_contents"]`

This requires:
- The renderer to support changing expected sections mid-operation
- The command handler to signal the transition after confirmation
- Proper cleanup of Phase 1 state before starting Phase 2

### Option 3: Update the Command Handler Pattern

The `update_stack_with_changeset_data` and `create_stack_with_changeset_data` functions need to be refactored to:
1. Create appropriate CLI context for each phase
2. Initialize new output managers with the correct operation context
3. Follow the established patterns from the architectural review document

## Implementation Plan

### Step 1: Update Expected Sections for ExecuteChangeset

First, fix the immediate issue with `ExecuteChangeset`:

```rust
// In interactive.rs:498
CfnOperation::ExecuteChangeset => vec!["command_metadata", "stack_definition", "stack_events", "live_stack_events", "stack_contents"],
```

This ensures that when changeset execution happens, all the expected sections are displayed.

### Step 2: Add Post-Confirmation Handler Support

The renderer should handle phase transitions after confirmation using a post-confirmation handler pattern:

```rust
impl InteractiveRenderer {
    /// Render confirmation prompt
    async fn render_confirmation_prompt(&mut self, mut request: ConfirmationRequest) -> Result<()> {
        // ... existing confirmation logic ...
        
        let confirmed = /* get user confirmation */;
        
        // Send response back to command handler
        if let Some(response_tx) = request.response_tx.take() {
            let _ = response_tx.send(confirmed);
        }
        
        // Handle post-confirmation actions based on key
        if confirmed {
            if let Some(key) = &request.key {
                match key.as_str() {
                    "execute_changeset" => self.post_confirmation_execute_changeset(),
                    _ => {} // No special handling for other keys
                }
            }
        }
        
        Ok(())
    }
    
    /// Post-confirmation handler for changeset execution
    fn post_confirmation_execute_changeset(&mut self) {
        // Clear current operation state
        self.cleanup_operation();
        
        // Set up expected sections for execution phase
        self.expected_sections = vec!["command_metadata", "stack_definition", "stack_events", "live_stack_events", "stack_contents"];
        self.next_section_index = 0;
        
        // Start first section of execution phase
        self.start_next_section();
    }
}
```

This approach:
- Keeps confirmation logic centralized
- Allows for different post-confirmation behaviors based on the confirmation key
- Maintains clean separation of concerns
- Is easily extensible for other confirmation types

### Step 3: Update CreateOrUpdate Expected Sections

Currently, `CreateOrUpdate` has static sections that don't work for changeset mode. We need to make it conditional:

```rust
// In interactive.rs setup_operation method
CfnOperation::CreateOrUpdate => {
    // Check if --changeset flag is set by examining the CLI context
    if let Commands::CreateOrUpdate(args) = &cli.command {
        if args.changeset {
            // Phase 1: Changeset creation
            vec!["command_metadata", "changeset_result", "confirmation"]
        } else {
            // Regular create-or-update flow
            vec!["command_metadata", "stack_change_details", "stack_definition", "live_stack_events", "stack_contents"]
        }
    } else {
        // Fallback (shouldn't happen)
        vec!["command_metadata", "stack_change_details", "stack_definition", "live_stack_events", "stack_contents"]
    }
}
```

### Step 4: Update Confirmation Request Keys

The changeset helper functions need to use the proper key when requesting confirmation:

```rust
// In update_stack_with_changeset_data and create_stack_with_changeset_data
let confirmed = if args.yes {
    true
} else {
    output_manager.request_confirmation_with_key(
        "Do you want to execute this changeset now?".to_string(),
        "execute_changeset".to_string()
    ).await?
};
```

This ensures the renderer knows this is a changeset execution confirmation and can handle the phase transition accordingly.

### Step 5: Refactor Changeset Helper Functions

The `update_stack_with_changeset_data` and `create_stack_with_changeset_data` functions need to be refactored to follow the data-driven architecture pattern:

1. **For changeset creation phase:**
   - Use the existing output manager with CreateOrUpdate context
   - Send appropriate data (CommandMetadata, ChangeSetResult, ConfirmationPrompt)
   - The renderer will handle sections based on the conditional logic

2. **For changeset execution phase (after confirmation):**
   - Continue using the same output manager (now transitioned by post-confirmation handler)
   - Send execution phase data (CommandMetadata, StackDefinition, etc.)
   - The renderer will display using the new expected sections

### Step 3: Ensure Proper Data Collection

Both phases need to collect and send the appropriate data:

**Phase 1 (Create Changeset):**
- CommandMetadata
- ChangeSetResult (with console URL, pending changesets, next steps)
- ConfirmationPrompt (asking "Do you want to execute this changeset now?")

**Phase 2 (Execute Changeset):**
- CommandMetadata (new context)
- StackDefinition
- StackEvents (previous events)
- NewStackEvents (live monitoring)
- StackContents
- FinalCommandSummary

## Testing Requirements

1. Test `create-or-update --changeset` for new stack creation
2. Test `create-or-update --changeset` for existing stack update
3. Test `create-or-update --changeset` with "No" response to confirmation
4. Verify all sections appear in the correct order
5. Verify spinners and timing work correctly for each phase
6. Test with both interactive and plain output modes

## Benefits

1. **Consistent Output:** Users will see the same output format regardless of how changesets are created
2. **Proper Section Ordering:** All data will be displayed in the expected order
3. **Better UX:** Clear separation between changeset creation and execution phases
4. **Maintainability:** Reuses existing, well-tested patterns for changeset operations