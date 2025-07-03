# Interactive Confirmation Prompts in Data-Driven Output Architecture

**Date:** 2025-07-02  
**Purpose:** Design confirmation prompts that maintain clean separation between command handlers and renderers

## Problem Statement

Currently, confirmation prompts are handled inconsistently across CFN operations:

1. **`delete_stack.rs:186`** - TODO comment indicates confirmation step not implemented
2. **`create_or_update.rs:217-218`** - Warning message about interactive confirmation not implemented  
3. **`update_stack.rs:217-222`** - Direct console I/O that violates architecture (uses `println!` and `stdin`)

The current `update_stack.rs` approach breaks our data-driven architecture by having command handlers perform console I/O directly.

## Design Principles

Following our established data-driven architecture:

### Command Handler Responsibilities
- Determine **when** confirmation is needed based on operation logic
- Provide the confirmation **message** to display
- **NEVER** perform direct console I/O or user interaction

### Renderer Responsibilities  
- Handle **how** to display confirmation prompts (formatting, positioning)
- Manage actual console I/O (reading stdin, displaying prompts)
- Handle different interaction modes (interactive vs plain vs json)
- Coordinate prompt display with ongoing spinners

## Data Structures

### Public API - Simple

The public API remains simple:
- **Input**: `String` message 
- **Output**: `bool` response (true = confirmed, false = declined)

### Internal Implementation

```rust
// Add to OutputData enum
#[derive(Clone, Debug)]
pub enum OutputData {
    // ... existing variants ...
    ConfirmationPrompt(ConfirmationRequest),
}

#[derive(Clone, Debug)]
pub struct ConfirmationRequest {
    pub message: String,
    pub response_tx: oneshot::Sender<bool>,
    pub key: Option<String>, // Optional key for multiple confirmations
}

## Implementation Approach

### 1. Simple API with Section Integration

Keep the clean `request_confirmation(message) -> bool` API while integrating with the section system internally:

```rust
pub async fn delete_stack(cli: &Cli, args: &DeleteArgs) -> Result<i32> {
    // ... existing data collection in parallel ...
    
    // Process all pre-confirmation sections
    output_manager.stop().await?;
    
    // Simple confirmation API (section integration is hidden)
    let confirmed = if args.yes {
        true
    } else {
        let message = format!("Are you sure you want to DELETE the stack {}?", stack_name);
        output_manager.request_confirmation(message).await?
    };
    
    if !confirmed {
        return Ok(130); // INTERRUPT exit code
    }
    
    // Continue with post-confirmation sections (e.g., live events)...
}
```

### 2. Output Manager Interface

The `DynamicOutputManager` keeps the simple API but coordinates with section system:

```rust
impl DynamicOutputManager {
    /// Request user confirmation and return whether user confirmed
    pub async fn request_confirmation(&mut self, message: String) -> Result<bool> {
        self.request_confirmation_impl(message, None).await
    }
    
    /// Request user confirmation with a specific section key
    pub async fn request_confirmation_with_key(&mut self, message: String, key: String) -> Result<bool> {
        self.request_confirmation_impl(message, Some(key)).await
    }
    
    /// Internal implementation for confirmation requests
    async fn request_confirmation_impl(&mut self, message: String, key: Option<String>) -> Result<bool> {
        // Create oneshot channel internally
        let (response_tx, response_rx) = oneshot::channel();
        
        // Create confirmation request with channel
        let confirmation = OutputData::ConfirmationPrompt(ConfirmationRequest {
            message,
            response_tx,
            key,
        });
        
        // Send through normal rendering system (integrates with sections)
        self.render(confirmation).await?;
        
        // Wait for response from renderer
        response_rx.await.map_err(|_| anyhow::anyhow!("Confirmation response channel closed"))
    }
}
```

### 3. Section Integration Details

#### Expected Sections Update

For operations requiring confirmation, add it to the section flow:

```rust
// In InteractiveRenderer::get_expected_sections()
CfnOperation::DeleteStack => vec![
    "command_metadata", 
    "stack_definition", 
    "previous_stack_events", 
    "stack_contents",
    "confirmation",        // Default confirmation section
    "live_stack_events"
],
```

#### Section Key Handling

Support multiple confirmations using dynamic section keys:

```rust
// In InteractiveRenderer::get_section_key()
OutputData::ConfirmationPrompt(ref request) => {
    match &request.key {
        Some(key) => format!("confirmation_{}", key),
        None => "confirmation".to_string(),
    }
}
```

This allows operations to have multiple confirmation points:
- `"confirmation"` - Default section for primary confirmation
- `"confirmation_changeset"` - Secondary confirmation for changeset execution
- `"confirmation_resources"` - Confirmation for specific resource changes

#### Section Titles

Confirmation sections should have no title but maintain visual separation:

```rust
// In InteractiveRenderer::get_section_title()
fn get_section_title(&self, section_key: &str) -> String {
    match section_key {
        "confirmation" => String::new(), // No title for confirmation
        key if key.starts_with("confirmation_") => String::new(), // No title for any confirmation
        // ... other section titles
        _ => format!("{}:", section_key), // Default title format
    }
}

// In section rendering logic
fn render_section_with_title(&mut self, section_key: &str, content: OutputData) -> Result<()> {
    let title = self.get_section_title(section_key);
    
    if title.is_empty() {
        // For confirmation sections: just add blank line separation
        if !self.is_first_section() {
            self.writeln("")?; // Blank line before confirmation
        }
    } else {
        // Normal sections: show title
        self.writeln(&title)?;
    }
    
    // Render the actual content...
}
```

#### Section Management

The InteractiveRenderer's section system will handle confirmation naturally:

- **Buffering**: If confirmation data arrives early, it's buffered until its turn
- **Ordering**: Confirmation always appears after stack info but before live events
- **Spinner Management**: Current section spinner is cleared before prompting
- **Abort Handling**: If user declines, skip remaining sections

### 4. Renderer Implementation

#### InteractiveRenderer Section Handler

```rust
impl InteractiveRenderer {
    // Add to render_output_data() match statement
    fn render_confirmation_prompt(&mut self, request: ConfirmationRequest) -> Result<()> {
        // Clear any active spinner (standard pattern for all sections)
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }
        
        let confirmed = if !self.options.enable_ansi_features {
            // Plain mode: show message but don't interact
            self.writeln(&format!("CONFIRMATION REQUIRED: {}", request.message))?;
            self.writeln("Use --yes flag to proceed automatically in non-interactive mode")?;
            false // Always decline in non-interactive mode for safety
        } else {
            // Interactive mode: prompt user
            print!("? {} (y/N) ", request.message);
            io::stdout().flush()?;
            
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim().to_lowercase();
            
            matches!(input.as_str(), "y" | "yes")
        };
        
        // Send response back to command handler via channel
        let _ = request.response_tx.send(confirmed);
        
        Ok(())
    }
}

#### JsonRenderer

```rust
impl JsonRenderer {
    fn render_confirmation_prompt(&mut self, request: ConfirmationRequest) -> Result<()> {
        // JSON mode: output confirmation event but don't interact
        let confirmation_event = serde_json::json!({
            "type": "confirmation_required",
            "message": request.message,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "response": "declined_non_interactive"
        });
        
        self.writeln(&confirmation_event.to_string())?;
        
        // Send response back to command handler via channel
        let _ = request.response_tx.send(false); // Always decline in JSON mode
        
        Ok(())
    }
}
```

## Usage Examples

Based on iidy-js reference implementation:

### Delete Stack Confirmation

```rust
// In delete_stack.rs (exact iidy-js message from deleteStack.ts:53)
let confirmed = if args.yes {
    true
} else {
    let message = format!("Are you sure you want to DELETE the stack {}?", stack_name);
    output_manager.request_confirmation(message).await?
};

if !confirmed {
    return Ok(130); // INTERRUPT exit code
}
```

### Changeset Execution Confirmation

```rust
// In update_stack.rs changeset mode (exact iidy-js message from index.ts:183)
let confirmed = if args.yes {
    true
} else {
    output_manager.request_confirmation("Do you want to execute this changeset now?".to_string()).await?
};

if !confirmed {
    return Ok(130); // INTERRUPT exit code
}
```

## Benefits

1. **Simple and Clean**: Matches iidy-js pattern exactly - just a message string and boolean response
2. **Architecture Compliant**: Command handlers determine when/what to confirm, renderers handle the interaction
3. **Consistent UX**: All confirmations use the same display patterns across operations
4. **Flag Integration**: Respects `--yes` flag consistently (command handlers check it first)
5. **Safe Defaults**: Defaults to "No" for safety, non-interactive modes (plain/json) always decline
6. **Exit Code Consistency**: Uses 130 (INTERRUPT) when user declines, matching iidy-js
7. **Testable**: Simple interface can be easily mocked for tests

## Implementation Steps

1. Add `ConfirmationPrompt` to `OutputData` enum and `ConfirmationRequest` struct
2. Add `request_confirmation` methods to `DynamicOutputManager` 
3. Implement `render_confirmation_prompt` in `InteractiveRenderer` and `JsonRenderer`
4. Update section handling (`get_section_key`, `get_section_title`) for confirmations
5. Update `delete_stack.rs` to use confirmation system (replace TODO at line 186)
6. Update `create_or_update.rs` to use confirmation system (replace warning at lines 217-218)
7. Update `update_stack.rs` to use confirmation system (replace direct I/O at lines 217-222)
8. Add tests for confirmation response parsing
9. Ensure all operations return correct exit codes (0=success, 1=failure, 130=interrupt)

This simplified design maintains our architectural principles while exactly matching the iidy-js confirmation pattern.