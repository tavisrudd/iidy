# Data-Driven Output Architecture

**Date:** 2025-06-17  
**Purpose:** Clean separation of data collection from presentation using structs + traits

## Overview

Instead of trait methods tied to specific AWS operations, we define data structs that capture exactly what iidy-js displays, then implement rendering traits for each output mode.

**Important Note on iidy-js Reference Implementation:**
We are only trying to match the external behavior (output format, timing, user experience) of iidy-js. The original iidy-js has a very different, more direct and less abstracted architecture. When reading iidy-js code for reference, we must NOT try to copy it directly. Instead, we need to:
- Infer what the intent and desired behavior is
- Understand the user-facing output patterns  
- Implement equivalent functionality in a way that fits our data-driven, section-based architecture
- Maintain clean separation between command handlers and renderers

## Core Architectural Principles

### 1. **Command Handlers vs Renderers Separation**

**Command Handlers (src/cfn/*):**
- Responsible for AWS API call sequencing and data collection
- Push structured data to `DynamicOutputManager` using `OutputData` enum
- Handle dependencies between API operations (e.g., get stack ID before deletion)
- **NEVER** make formatting choices, wording choices, or know about sections/spinners
- **NEVER** decide section titles, spinner text, or display logic
- Focus purely on: API calls → data conversion → push to output manager

**Renderers (src/output/renderers/*):**
- Handle all presentation logic, section management, and spinner coordination
- Define expected sections for each operation in `get_expected_sections()`
- Manage spinner lifecycle: start, update, clear on section transitions
- Handle display coordination issues (spinner cancellation, section ordering)
- Responsible for titles, formatting, colors, layout

### 2. **Predefined Sections Architecture**

All operations use predefined sections defined in `interactive.rs`:
```rust
fn get_expected_sections(&self, operation: &CfnOperation) -> Vec<&'static str> {
    match operation {
        CfnOperation::CreateStack => vec!["stack_definition", "live_stack_events", "stack_contents"],
        CfnOperation::DeleteStack => vec!["stack_definition", "previous_stack_events", "stack_contents", "live_stack_events"],
        CfnOperation::WatchStack => vec!["stack_definition", "previous_stack_events", "live_stack_events", "stack_contents"],
        // etc.
    }
}
```

### 3. **Spinner Management**

**Automatic Spinner Lifecycle:**
- Renderers automatically start spinners for each section
- Spinners are cleared when:
  - Moving to next section 
  - Receiving `OperationComplete` OutputData
  - Receiving `InactivityTimeout` OutputData
  - Any terminal condition for live_stack_events section

**Command handlers NEVER:**
- Start/stop spinners directly
- Know about spinner state or timing
- Handle spinner cancellation logic

### 4. **Live Events Coordination**

**Live events async task management:**
- Renderers handle timing task lifecycle
- Automatic cleanup on `OperationComplete` or `InactivityTimeout`
- Command handlers only push `NewStackEvents` data
- Renderers coordinate spinner updates with live event display

### 5. **Section Cancellation and Skipping**

**Renderer Responsibility:**
- Skip remaining sections when `OperationComplete` has `skip_remaining_sections: true`
- Handle stack deletion/rollback cases where some sections become invalid (e.g., skip `stack_contents` if stack was deleted)
- Cancel spinners and clean up async tasks for skipped sections
- Decide which sections to skip based on operation outcome

**Command Handler Responsibility:**
- Avoid making AWS API calls that would fail due to stack state
- Send `OperationComplete` with appropriate `skip_remaining_sections` flag
- Stop API polling/monitoring when terminal state reached
- Focus on AWS API logic, not display coordination

### 6. **Parallel API Calls and Section Ordering**

**Command Handler Optimization:**
- May make parallel AWS API calls for latency optimization (e.g., concurrent `describe_stack` and `list_stack_events`)
- Can send section data out of order as API calls complete
- Should not worry about display order or timing

**Renderer Responsibility:**
- Handle out-of-order section data arrival
- Buffer pending sections and render in expected order
- Wait for required sections before moving to next section
- Coordinate proper section transitions regardless of API call timing

## Data Structures

### Core Display Data

```rust
// Core AWS CloudFormation stack event (matches AWS SDK structure)
#[derive(Clone, Debug)]
pub struct StackEvent {
    pub event_id: String,
    pub stack_id: String,
    pub stack_name: String,
    pub logical_resource_id: String,
    pub physical_resource_id: Option<String>,
    pub resource_type: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub resource_status: String,
    pub resource_status_reason: Option<String>,
    pub resource_properties: Option<String>,
    pub client_request_token: Option<String>,
}

// Individual change within a changeset
#[derive(Clone, Debug)]
pub struct ChangeInfo {
    pub action: String, // Add, Modify, Remove
    pub logical_resource_id: String,
    pub resource_type: String,
    pub replacement: Option<String>, // True, False, Conditional
    pub details: Vec<ChangeDetail>,
}

#[derive(Clone, Debug)]
pub struct ChangeDetail {
    pub target: String,
    pub evaluation: Option<String>,
    pub change_source: Option<String>,
    pub causing_entity: Option<String>,
}

// Command metadata shown at start of create/update operations
#[derive(Clone, Debug)]
pub struct CommandMetadata {
    pub cfn_operation: String,
    pub iidy_environment: String,
    pub region: String,
    pub profile: Option<String>,
    pub cli_arguments: HashMap<String, String>,
    pub iam_service_role: Option<String>,
    pub current_iam_principal: String,
    pub iidy_version: String,
    pub primary_token: TokenInfo,           // Always present (from token management system)
    pub derived_tokens: Vec<TokenInfo>,     // Sub-tokens for multi-step operations
}

// Token information (from existing token management system)
#[derive(Clone, Debug)]
pub struct TokenInfo {
    pub value: String,
    pub source: TokenSource,
    pub operation_id: String,
}

#[derive(Clone, Debug)]
pub enum TokenSource {
    UserProvided,
    AutoGenerated,
    Derived { from: String, step: String },
}

// Stack definition details (from summarizeStackDefinition)
#[derive(Clone, Debug)]
pub struct StackDefinition {
    pub name: String,
    pub stackset_name: Option<String>, // If StackSet
    pub description: Option<String>,
    pub status: String,
    pub capabilities: Vec<String>,
    pub service_role: Option<String>,
    pub tags: HashMap<String, String>,
    pub parameters: HashMap<String, String>,
    pub disable_rollback: bool,
    pub termination_protection: bool,
    pub creation_time: Option<DateTime<Utc>>,
    pub last_updated_time: Option<DateTime<Utc>>,
    pub timeout_in_minutes: Option<i32>,
    pub notification_arns: Vec<String>,
    pub stack_policy: Option<String>,
    pub arn: String,
    pub console_url: String,
    pub region: String,
}

// Stack events list with metadata  
#[derive(Clone, Debug)]
pub struct StackEventsDisplay {
    pub title: String, // e.g., "Previous Stack Events (max 10):" or "Live Stack Events (2s poll):"
    pub events: Vec<StackEventWithTiming>,
    pub truncated: Option<TruncationInfo>,
}

#[derive(Clone, Debug)]
pub struct StackEventWithTiming {
    pub event: StackEvent,
    pub duration_seconds: Option<u64>, // Time to completion
}

#[derive(Clone, Debug)]
pub struct TruncationInfo {
    pub shown: usize,
    pub total: usize,
}

// Stack contents (from summarizeStackContents)
#[derive(Clone, Debug)]
pub struct StackContents {
    pub resources: Vec<StackResourceInfo>,
    pub outputs: Vec<StackOutputInfo>,
    pub exports: Vec<StackExportInfo>,
    pub current_status: StackStatusInfo,
    pub pending_changesets: Vec<ChangeSetInfo>,
}

#[derive(Clone, Debug)]
pub struct StackResourceInfo {
    pub logical_id: String,
    pub resource_type: String,
    pub physical_id: String,
}

#[derive(Clone, Debug)]
pub struct StackOutputInfo {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug)]
pub struct StackExportInfo {
    pub name: String,
    pub value: String,
    pub imported_by: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct StackStatusInfo {
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ChangeSetInfo {
    pub name: String,
    pub status: String,
    pub status_reason: Option<String>,
    pub description: Option<String>,
    pub creation_time: DateTime<Utc>,
    pub changes: Vec<ChangeInfo>, // From summarizeChangeSet
}

// Live operation status updates
#[derive(Clone, Debug)]
pub struct StatusUpdate {
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

// Final operation result
#[derive(Clone, Debug)]
pub struct CommandResult {
    pub success: bool,
    pub message: Option<String>,
    pub elapsed_seconds: u64,
}

// List stacks display data
#[derive(Clone, Debug)]
pub struct StackListDisplay {
    pub header: String,
    pub stacks: Vec<StackListEntry>,
}

#[derive(Clone, Debug)]
pub struct StackListEntry {
    pub name: String,
    pub status: String,
    pub timestamp: DateTime<Utc>,
    pub lifecycle_icon: Option<String>, // 🔒, ∞, ♺
    pub environment_color: Option<EnvironmentColor>,
    pub tags: HashMap<String, String>,
    pub failure_reason: Option<String>,
    pub is_stackset_instance: bool,
    pub stackset_info: Option<String>,
}

#[derive(Clone, Debug)]
pub enum EnvironmentColor {
    Production,    // Red
    Integration,   // xterm(75)  
    Development,   // xterm(194)
}

// Changeset creation result
#[derive(Clone, Debug)]
pub struct ChangeSetCreationResult {
    pub changeset_name: String,
    pub stack_name: String,
    pub has_changes: bool,
    pub console_url: String,
    pub is_new_stack: bool,
    pub exec_command: Option<String>,
}

// Error display data
#[derive(Clone, Debug)]
pub struct ErrorInfo {
    pub message: String,
    pub context: Option<String>,
    pub timestamp: DateTime<Utc>,
}
```

## Output Mode Traits

### Core Rendering Trait

```rust
use async_trait::async_trait;

#[async_trait]
pub trait OutputRenderer: Send + Sync {
    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()>;
    async fn render_stack_definition(&mut self, data: &StackDefinition, show_times: bool) -> Result<()>;
    async fn render_stack_events(&mut self, data: &StackEventsDisplay) -> Result<()>;
    async fn render_stack_contents(&mut self, data: &StackContents) -> Result<()>;
    async fn render_status_update(&mut self, data: &StatusUpdate) -> Result<()>;
    async fn render_command_result(&mut self, data: &CommandResult) -> Result<()>;
    async fn render_stack_list(&mut self, data: &StackListDisplay) -> Result<()>;
    async fn render_changeset_result(&mut self, data: &ChangeSetCreationResult) -> Result<()>;
    async fn render_error(&mut self, data: &ErrorInfo) -> Result<()>;
    
    // Control methods
    async fn init(&mut self) -> Result<()>;
    async fn cleanup(&mut self) -> Result<()>;
}
```

### Specific Mode Implementations

```rust
// Interactive mode - exact iidy-js formatting
pub struct InteractiveRenderer {
    color_context: ColorContext,
    terminal_width: usize,
    spinner: Option<ProgressManager>,
}

impl InteractiveRenderer {
    fn format_section_heading(&self, text: &str) -> String {
        text.color(UserDefined(255)).bold().to_string()
    }
    
    fn format_section_entry(&self, label: &str, value: &str) -> String {
        format!(" {}{}\n",
            label.color(UserDefined(255)).format!("{:<width$} ", width = COLUMN2_START - 1),
            value
        )
    }
    
    fn colorize_resource_status(&self, status: &str, padding: usize) -> String {
        // Exact iidy-js colorizeResourceStatus logic
        let padded = format!("{:<width$}", status, width = padding);
        
        if FAILED.contains(&status) {
            padded.bright_red().to_string()
        } else if SKIPPED.contains(&status) {
            padded.blue().to_string()
        } else if COMPLETE.contains(&status) {
            padded.green().to_string()
        } else if IN_PROGRESS.contains(&status) {
            padded.yellow().to_string()
        } else {
            padded
        }
    }
}

#[async_trait]
impl OutputRenderer for InteractiveRenderer {
    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()> {
        println!(); // blank line
        println!("{}", self.format_section_heading("Command Metadata:"));
        
        print!("{}", self.format_section_entry("CFN Operation:", &data.cfn_operation.magenta().to_string()));
        print!("{}", self.format_section_entry("iidy Environment:", &data.iidy_environment.magenta().to_string()));
        print!("{}", self.format_section_entry("Region:", &data.region.magenta().to_string()));
        
        if let Some(profile) = &data.profile {
            print!("{}", self.format_section_entry("Profile:", &profile.magenta().to_string()));
        }
        
        let cli_args = pretty_format_small_map(&data.cli_arguments);
        print!("{}", self.format_section_entry("CLI Arguments:", &cli_args.truecolor(128, 128, 128).to_string()));
        
        let service_role = data.iam_service_role.as_deref().unwrap_or("None");
        print!("{}", self.format_section_entry("IAM Service Role:", &service_role.truecolor(128, 128, 128).to_string()));
        
        print!("{}", self.format_section_entry("Current IAM Principal:", &data.current_iam_principal.truecolor(128, 128, 128).to_string()));
        print!("{}", self.format_section_entry("iidy Version:", &data.iidy_version.truecolor(128, 128, 128).to_string()));
        
        println!();
        
        // Display token information (matching token management system format)
        self.render_token_info(&data.primary_token, &data.derived_tokens);
        
        println!();
        Ok(())
    }
    
    fn render_token_info(&self, primary_token: &TokenInfo, derived_tokens: &[TokenInfo]) {
        // Display primary token with appropriate icon and message
        match &primary_token.source {
            TokenSource::UserProvided => {
                println!("🔑 Using provided idempotency token {}", primary_token.value);
            },
            TokenSource::AutoGenerated => {
                println!("🎲 Generated idempotency token {} (save this for retries)", primary_token.value);
            },
            TokenSource::Derived { from, step } => {
                println!("🔗 Using derived token {} (from {} for step {})", primary_token.value, from, step);
            },
        }
        
        // Display derived tokens for multi-step operations
        for token in derived_tokens {
            if let TokenSource::Derived { from, step } = &token.source {
                println!("   🔄 Step '{}' token {} (derived from {})", step, token.value, &from[..8]);
            }
        }
    }
    
    async fn render_stack_definition(&mut self, data: &StackDefinition, show_times: bool) -> Result<()> {
        println!("{}", self.format_section_heading("Stack Details:"));
        
        // Name (handle StackSet)
        if let Some(stackset_name) = &data.stackset_name {
            print!("{}", self.format_section_entry("Name (StackSet):", 
                &format!("{} {}", 
                    data.name.truecolor(128, 128, 128),
                    stackset_name.magenta()
                )
            ));
        } else {
            print!("{}", self.format_section_entry("Name:", &data.name.magenta().to_string()));
        }
        
        // Description
        if let Some(description) = &data.description {
            let desc_color = if data.name.starts_with("StackSet") {
                description.magenta().to_string()
            } else {
                description.truecolor(128, 128, 128).to_string()
            };
            print!("{}", self.format_section_entry("Description:", &desc_color));
        }
        
        // Status (colorized)
        print!("{}", self.format_section_entry("Status", 
            &self.colorize_resource_status(&data.status, MIN_STATUS_PADDING)));
        
        // Capabilities
        let capabilities = if data.capabilities.is_empty() {
            "None".to_string()
        } else {
            data.capabilities.join(", ")
        };
        print!("{}", self.format_section_entry("Capabilities:", &capabilities.truecolor(128, 128, 128).to_string()));
        
        // Service Role
        let service_role = data.service_role.as_deref().unwrap_or("None");
        print!("{}", self.format_section_entry("Service Role:", &service_role.truecolor(128, 128, 128).to_string()));
        
        // Tags
        let tags_str = pretty_format_tags(&data.tags);
        print!("{}", self.format_section_entry("Tags:", &tags_str.truecolor(128, 128, 128).to_string()));
        
        // Parameters  
        let params_str = pretty_format_small_map(&data.parameters);
        print!("{}", self.format_section_entry("Parameters:", &params_str.truecolor(128, 128, 128).to_string()));
        
        // DisableRollback
        print!("{}", self.format_section_entry("DisableRollback:", 
            &data.disable_rollback.to_string().truecolor(128, 128, 128).to_string()));
        
        // TerminationProtection
        let protection_text = format!("{}{}", 
            data.termination_protection.to_string().truecolor(128, 128, 128),
            if data.termination_protection { " 🔒" } else { "" }
        );
        print!("{}", self.format_section_entry("TerminationProtection:", &protection_text));
        
        // Times (conditional)
        if show_times {
            if let Some(creation_time) = data.creation_time {
                print!("{}", self.format_section_entry("Creation Time:", 
                    &render_timestamp(creation_time).truecolor(128, 128, 128).to_string()));
            }
            if let Some(last_updated_time) = data.last_updated_time {
                print!("{}", self.format_section_entry("Last Update Time:", 
                    &render_timestamp(last_updated_time).truecolor(128, 128, 128).to_string()));
            }
        }
        
        // Timeout
        if let Some(timeout) = data.timeout_in_minutes {
            print!("{}", self.format_section_entry("Timeout In Minutes:", 
                &timeout.to_string().truecolor(128, 128, 128).to_string()));
        }
        
        // NotificationARNs
        let notification_arns = if data.notification_arns.is_empty() {
            "None".to_string()
        } else {
            data.notification_arns.join(", ")
        };
        print!("{}", self.format_section_entry("NotificationARNs:", &notification_arns.truecolor(128, 128, 128).to_string()));
        
        // Stack Policy
        if let Some(policy) = &data.stack_policy {
            print!("{}", self.format_section_entry("Stack Policy Source:", &policy.truecolor(128, 128, 128).to_string()));
        }
        
        // ARN
        print!("{}", self.format_section_entry("ARN:", &data.arn.truecolor(128, 128, 128).to_string()));
        
        // Console URL
        print!("{}", self.format_section_entry("Console URL:", &data.console_url.truecolor(128, 128, 128).to_string()));
        
        Ok(())
    }
    
    async fn render_stack_events(&mut self, data: &StackEventsDisplay) -> Result<()> {
        println!("{}", self.format_section_heading(&data.title));
        
        if data.events.is_empty() {
            return Ok(());
        }
        
        // Calculate padding from all events
        let status_padding = calc_padding(&data.events, |e| &e.event.resource_status);
        
        for event_with_timing in &data.events {
            display_stack_event(&event_with_timing.event, status_padding, event_with_timing.duration_seconds);
        }
        
        // Show truncation message
        if let Some(truncation) = &data.truncated {
            println!("{}", 
                format!(" {} of {} total events shown", truncation.shown, truncation.total)
                    .truecolor(128, 128, 128)
            );
        }
        
        Ok(())
    }
    
    async fn render_stack_contents(&mut self, data: &StackContents) -> Result<()> {
        // Stack Resources
        if !data.resources.is_empty() {
            println!("{}", self.format_section_heading("Stack Resources:"));
            let id_padding = calc_padding(&data.resources, |r| &r.logical_id);
            let type_padding = calc_padding(&data.resources, |r| &r.resource_type);
            
            for resource in &data.resources {
                println!("{} {} {}",
                    format_logical_id(&format!(" {:<width$}", resource.logical_id, width = id_padding)),
                    format!("{:<width$}", resource.resource_type, width = type_padding).truecolor(128, 128, 128),
                    resource.physical_id.truecolor(128, 128, 128)
                );
            }
        }
        
        println!();
        
        // Stack Outputs
        print!("{}", self.format_section_heading("Stack Outputs:"));
        if data.outputs.is_empty() {
            println!(" {}", "None".truecolor(128, 128, 128));
        } else {
            println!();
            let key_padding = calc_padding(&data.outputs, |o| &o.key);
            for output in &data.outputs {
                println!("{} {}",
                    format_logical_id(&format!(" {:<width$}", output.key, width = key_padding)),
                    output.value.truecolor(128, 128, 128)
                );
            }
        }
        
        // Stack Exports
        if !data.exports.is_empty() {
            println!();
            println!("{}", self.format_section_heading("Stack Exports:"));
            let name_padding = calc_padding(&data.exports, |e| &e.name);
            
            for export in &data.exports {
                println!("{} {}",
                    format_logical_id(&format!(" {:<width$}", export.name, width = name_padding)),
                    export.value.truecolor(128, 128, 128)
                );
                
                for import in &export.imported_by {
                    println!("  {}", format!("imported by {}", import).truecolor(128, 128, 128));
                }
            }
        }
        
        println!();
        
        // Current Stack Status
        println!("{} {} {}",
            self.format_section_heading(&format!("{:<width$}", "Current Stack Status:", width = COLUMN2_START)),
            self.colorize_resource_status(&data.current_status.status, MIN_STATUS_PADDING),
            data.current_status.reason.as_deref().unwrap_or("")
        );
        
        // Pending Changesets (if any) - would call render_changesets
        
        Ok(())
    }
    
    async fn render_command_result(&mut self, data: &CommandResult) -> Result<()> {
        if data.success {
            println!("{} {} 👍",
                self.format_section_heading(&format!("{:<width$}", "Command Summary:", width = COLUMN2_START)),
                "Success".black().on_bright_green()
            );
        } else {
            println!("{} {} (╯°□°）╯︵ ┻━┻ Fix and try again.",
                self.format_section_heading(&format!("{:<width$}", "Command Summary:", width = COLUMN2_START)),
                "Failure".on_bright_red()
            );
        }
        Ok(())
    }
    
    // ... implement other render methods
}

// Plain text mode - no colors, no spinners
pub struct PlainTextRenderer {
    // Simple text output
}

// JSON mode - structured data
pub struct JsonRenderer {
    // Output JSON lines
}

// TUI mode - full screen interface
pub struct TuiRenderer {
    terminal: ratatui::Terminal<CrosstermBackend<std::io::Stdout>>,
}
```

## Dynamic Mode Switching

```rust
pub struct DynamicOutputManager {
    current_mode: OutputMode,
    current_renderer: Box<dyn OutputRenderer>,
    event_buffer: Vec<OutputData>,
    options: OutputOptions,
    keyboard_listener: Option<KeyboardListener>,
}

#[derive(Clone, Debug)]
pub enum OutputData {
    CommandMetadata(CommandMetadata),
    StackDefinition(StackDefinition, bool), // show_times flag
    StackEvents(StackEventsDisplay),
    StackContents(StackContents),
    StatusUpdate(StatusUpdate),
    CommandResult(CommandResult),
    StackList(StackListDisplay),
    ChangeSetResult(ChangeSetCreationResult),
    Error(ErrorInfo),
}

impl DynamicOutputManager {
    pub async fn render(&mut self, data: OutputData) -> Result<()> {
        // Buffer the data
        self.event_buffer.push(data.clone());
        
        // Render with current mode
        match data {
            OutputData::CommandMetadata(ref metadata) => {
                self.current_renderer.render_command_metadata(metadata).await?;
            }
            OutputData::StackDefinition(ref def, show_times) => {
                self.current_renderer.render_stack_definition(def, show_times).await?;
            }
            OutputData::StackEvents(ref events) => {
                self.current_renderer.render_stack_events(events).await?;
            }
            // ... other cases
        }
        
        // Check for mode switch
        self.check_mode_switch().await?;
        
        Ok(())
    }
    
    async fn switch_to_mode(&mut self, new_mode: OutputMode) -> Result<()> {
        // Clean up current renderer
        self.current_renderer.cleanup().await?;
        
        // Create new renderer
        self.current_renderer = new_mode.create_renderer(&self.options);
        self.current_renderer.init().await?;
        
        // Re-render all buffered data
        for data in &self.event_buffer {
            match data {
                OutputData::CommandMetadata(metadata) => {
                    self.current_renderer.render_command_metadata(metadata).await?;
                }
                // ... re-render all data types
            }
        }
        
        self.current_mode = new_mode;
        Ok(())
    }
}
```

## Usage in Commands

```rust
// In create_stack command
pub async fn create_stack_main(args: &Args) -> Result<i32> {
    let mut output = DynamicOutputManager::new(args.output_mode, args.output_options()).await?;
    
    // 1. Command metadata
    let metadata = collect_command_metadata(args).await?;
    output.render(OutputData::CommandMetadata(metadata)).await?;
    
    // 2. Stack operation
    let start_time = get_reliable_start_time().await;
    perform_create_stack().await?;
    
    // 3. Stack definition
    let stack_def = collect_stack_definition(&stack_name, &region, true).await?;
    output.render(OutputData::StackDefinition(stack_def, true)).await?;
    
    // 4. Previous events
    let previous_events = collect_stack_events(&stack_name, 10, "Previous Stack Events (max 10):").await?;
    output.render(OutputData::StackEvents(previous_events)).await?;
    
    // 5. Live events (with built-in spinner for interactive mode)
    watch_stack_with_output(&stack_name, start_time, &mut output).await?;
    
    // 6. Final summary
    let contents = collect_stack_contents(&stack_id).await?;
    output.render(OutputData::StackContents(contents)).await?;
    
    // 7. Command result
    let result = CommandResult { success: true, elapsed_seconds: calc_elapsed(start_time), message: None };
    output.render(OutputData::CommandResult(result)).await?;
    
    Ok(if result.success { 0 } else { 1 })
}
```

## Benefits

1. **Clean Separation**: Data collection vs presentation logic
2. **Type Safety**: Exact data structures match iidy-js display sections
3. **Mode Flexibility**: Each renderer can format data appropriately
4. **Easy Testing**: Mock data structures for output testing
5. **Extensibility**: Add new output modes without touching data logic
6. **Consistency**: Same data structures across all modes ensure consistency

This architecture makes the code much more maintainable and testable while preserving the ability to exactly match iidy-js output in interactive mode.

---

## Theme System Integration

### Theme Architecture

Based on the complete iidy-js implementation spec and the color theming design, we integrate themes into the output system by replacing hardcoded color constants with semantic theme definitions.

#### Theme Definition Structure

```rust
// src/output/theme.rs

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Theme {
    /// Auto-detect based on terminal background
    Auto,
    /// Dark terminal theme (exact iidy-js compatibility)  
    Dark,
    /// Light terminal theme (inverted for light backgrounds)
    Light,
    /// High contrast theme (accessibility focused)
    HighContrast,
}

/// Semantic color definitions for CloudFormation output
#[derive(Debug, Clone)]
pub struct OutputTheme {
    // Status Colors (CloudFormation-specific)
    pub status_in_progress: anstyle::Color,      // Yellow for IN_PROGRESS states
    pub status_complete: anstyle::Color,         // Green for COMPLETE states  
    pub status_failed: anstyle::Color,           // Red for FAILED states
    pub status_skipped: anstyle::Color,          // Blue for SKIPPED states
    
    // Text Role Colors
    pub section_heading: anstyle::Style,         // Bold white/black for headings
    pub section_label: anstyle::Color,           // White/black for labels
    pub primary_value: anstyle::Color,           // Magenta for primary values
    pub secondary_value: anstyle::Color,         // Gray for secondary text
    pub timestamp: anstyle::Color,               // Light gray for timestamps
    pub logical_id: anstyle::Color,              // Light gray for resource IDs
    pub muted_text: anstyle::Color,              // Dark gray for muted content
    
    // Environment-Specific Colors
    pub env_production: anstyle::Color,          // Red for production
    pub env_integration: anstyle::Color,         // Blue-ish for integration
    pub env_development: anstyle::Color,         // Yellow-ish for development
    
    // Infrastructure Element Colors
    pub console_url: anstyle::Color,             // Gray for URLs
    pub arn: anstyle::Color,                     // Gray for ARNs
    pub token_generated: anstyle::Color,         // Color for generated tokens
    pub token_provided: anstyle::Color,          // Color for user-provided tokens
    pub token_derived: anstyle::Color,           // Color for derived tokens
    
    // Structural Colors
    pub success_bg: anstyle::Color,              // Success command background
    pub failure_bg: anstyle::Color,              // Failure command background
    pub spinner: anstyle::Color,                 // Spinner color
    
    // Layout Constants (theme-specific padding adjustments)
    pub column2_start: usize,                    // Column alignment
    pub default_status_padding: usize,          // Status field padding
    pub min_status_padding: usize,              // Minimum status padding
    pub max_padding: usize,                     // Maximum field padding
    pub resource_type_padding: usize,           // Resource type padding
    pub default_screen_width: usize,            // Assumed screen width
}

impl OutputTheme {
    /// Create theme for dark terminals (exact iidy-js compatibility)
    pub fn dark() -> Self {
        Self {
            // Status colors (exact iidy-js mapping)
            status_in_progress: anstyle::Color::Ansi(anstyle::AnsiColor::Yellow),
            status_complete: anstyle::Color::Ansi(anstyle::AnsiColor::Green),
            status_failed: anstyle::Color::Ansi(anstyle::AnsiColor::BrightRed),
            status_skipped: anstyle::Color::Ansi(anstyle::AnsiColor::Blue),
            
            // Text roles (exact iidy-js xterm codes)
            section_heading: anstyle::Style::new()
                .fg_color(Some(anstyle::Color::Ansi256(255))) // White
                .bold(),
            section_label: anstyle::Color::Ansi256(255),     // White
            primary_value: anstyle::Color::Ansi(anstyle::AnsiColor::Magenta),
            secondary_value: anstyle::Color::Rgb(anstyle::RgbColor(128, 128, 128)), // truecolor(128,128,128)
            timestamp: anstyle::Color::Ansi256(253),         // Light gray
            logical_id: anstyle::Color::Ansi256(252),        // Light gray
            muted_text: anstyle::Color::Rgb(anstyle::RgbColor(128, 128, 128)),
            
            // Environment colors (exact iidy-js xterm codes)
            env_production: anstyle::Color::Ansi(anstyle::AnsiColor::Red),
            env_integration: anstyle::Color::Ansi256(75),    // Blue-ish
            env_development: anstyle::Color::Ansi256(194),   // Yellow-ish
            
            // Infrastructure elements
            console_url: anstyle::Color::Rgb(anstyle::RgbColor(128, 128, 128)),
            arn: anstyle::Color::Rgb(anstyle::RgbColor(128, 128, 128)),
            token_generated: anstyle::Color::Ansi(anstyle::AnsiColor::Yellow),     // 🎲
            token_provided: anstyle::Color::Ansi(anstyle::AnsiColor::Green),       // 🔑
            token_derived: anstyle::Color::Ansi(anstyle::AnsiColor::Cyan),         // 🔗/🔄
            
            // Structural elements
            success_bg: anstyle::Color::Ansi(anstyle::AnsiColor::BrightGreen),
            failure_bg: anstyle::Color::Ansi(anstyle::AnsiColor::BrightRed),
            spinner: anstyle::Color::Ansi256(240),           // Dark gray
            
            // Layout (exact iidy-js constants)
            column2_start: 25,
            default_status_padding: 35,
            min_status_padding: 17,
            max_padding: 60,
            resource_type_padding: 40,
            default_screen_width: 130,
        }
    }
    
    /// Create theme for light terminals (inverted/adapted for light backgrounds)
    pub fn light() -> Self {
        Self {
            // Status colors (adapted for light background)
            status_in_progress: anstyle::Color::Ansi256(130),  // Darker orange/brown
            status_complete: anstyle::Color::Ansi256(28),      // Darker green
            status_failed: anstyle::Color::Ansi(anstyle::AnsiColor::Red),
            status_skipped: anstyle::Color::Ansi256(26),       // Darker blue
            
            // Text roles (dark colors for light background)
            section_heading: anstyle::Style::new()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Black)))
                .bold(),
            section_label: anstyle::Color::Ansi(anstyle::AnsiColor::Black),
            primary_value: anstyle::Color::Ansi256(90),       // Dark purple instead of magenta
            secondary_value: anstyle::Color::Ansi256(240),    // Medium gray
            timestamp: anstyle::Color::Ansi256(240),          // Medium gray
            logical_id: anstyle::Color::Ansi256(240),         // Medium gray
            muted_text: anstyle::Color::Ansi256(240),         // Medium gray
            
            // Environment colors (darker variants)
            env_production: anstyle::Color::Ansi256(124),     // Dark red
            env_integration: anstyle::Color::Ansi256(26),     // Dark blue
            env_development: anstyle::Color::Ansi256(130),    // Dark orange
            
            // Infrastructure elements (darker for visibility)
            console_url: anstyle::Color::Ansi256(240),
            arn: anstyle::Color::Ansi256(240),
            token_generated: anstyle::Color::Ansi256(130),    // Dark orange
            token_provided: anstyle::Color::Ansi256(28),      // Dark green
            token_derived: anstyle::Color::Ansi256(26),       // Dark blue
            
            // Structural elements
            success_bg: anstyle::Color::Ansi(anstyle::AnsiColor::Green),
            failure_bg: anstyle::Color::Ansi(anstyle::AnsiColor::Red),
            spinner: anstyle::Color::Ansi256(240),
            
            // Layout (same as dark)
            column2_start: 25,
            default_status_padding: 35,
            min_status_padding: 17,
            max_padding: 60,
            resource_type_padding: 40,
            default_screen_width: 130,
        }
    }
    
    /// Create high contrast theme (accessibility focused)
    pub fn high_contrast() -> Self {
        Self {
            // High contrast status colors
            status_in_progress: anstyle::Color::Ansi(anstyle::AnsiColor::BrightYellow),
            status_complete: anstyle::Color::Ansi(anstyle::AnsiColor::BrightGreen),
            status_failed: anstyle::Color::Ansi(anstyle::AnsiColor::BrightRed),
            status_skipped: anstyle::Color::Ansi(anstyle::AnsiColor::BrightBlue),
            
            // High contrast text
            section_heading: anstyle::Style::new()
                .fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::BrightWhite)))
                .bold(),
            section_label: anstyle::Color::Ansi(anstyle::AnsiColor::BrightWhite),
            primary_value: anstyle::Color::Ansi(anstyle::AnsiColor::BrightMagenta),
            secondary_value: anstyle::Color::Ansi(anstyle::AnsiColor::White),
            timestamp: anstyle::Color::Ansi(anstyle::AnsiColor::White),
            logical_id: anstyle::Color::Ansi(anstyle::AnsiColor::White),
            muted_text: anstyle::Color::Ansi(anstyle::AnsiColor::White),
            
            // High contrast environment colors
            env_production: anstyle::Color::Ansi(anstyle::AnsiColor::BrightRed),
            env_integration: anstyle::Color::Ansi(anstyle::AnsiColor::BrightBlue),
            env_development: anstyle::Color::Ansi(anstyle::AnsiColor::BrightYellow),
            
            // High contrast infrastructure
            console_url: anstyle::Color::Ansi(anstyle::AnsiColor::White),
            arn: anstyle::Color::Ansi(anstyle::AnsiColor::White),
            token_generated: anstyle::Color::Ansi(anstyle::AnsiColor::BrightYellow),
            token_provided: anstyle::Color::Ansi(anstyle::AnsiColor::BrightGreen),
            token_derived: anstyle::Color::Ansi(anstyle::AnsiColor::BrightCyan),
            
            // High contrast structural
            success_bg: anstyle::Color::Ansi(anstyle::AnsiColor::BrightGreen),
            failure_bg: anstyle::Color::Ansi(anstyle::AnsiColor::BrightRed),
            spinner: anstyle::Color::Ansi(anstyle::AnsiColor::BrightWhite),
            
            // Layout (same padding, may be adjusted for accessibility)
            column2_start: 25,
            default_status_padding: 35,
            min_status_padding: 17,
            max_padding: 60,
            resource_type_padding: 40,
            default_screen_width: 130,
        }
    }
    
    /// Auto-detect theme based on terminal background
    pub fn auto() -> Self {
        if Self::detect_dark_background() {
            Self::dark()
        } else {
            Self::light()
        }
    }
    
    /// Detect if terminal has dark background
    fn detect_dark_background() -> bool {
        // Multiple detection strategies
        
        // 1. Check COLORFGBG environment variable (format: "15;0" = white fg, black bg)
        if let Ok(colorfgbg) = std::env::var("COLORFGBG") {
            if let Some(bg) = colorfgbg.split(';').nth(1) {
                if let Ok(bg_color) = bg.parse::<u8>() {
                    // Colors 0-7 are typically dark, 8-15 are bright
                    return bg_color < 8;
                }
            }
        }
        
        // 2. Check terminal emulator
        if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
            match term_program.as_str() {
                "Apple_Terminal" => {
                    // macOS Terminal.app defaults to dark in recent versions
                    return true;
                }
                "iTerm.app" => {
                    // iTerm2 commonly uses dark themes
                    return true;
                }
                "vscode" => {
                    // VS Code integrated terminal often dark
                    return true;
                }
                _ => {}
            }
        }
        
        // 3. Check for common dark theme indicators
        for var in ["DARK_MODE", "THEME"] {
            if let Ok(value) = std::env::var(var) {
                let lower = value.to_lowercase();
                if lower.contains("dark") || lower.contains("black") {
                    return true;
                }
                if lower.contains("light") || lower.contains("white") {
                    return false;
                }
            }
        }
        
        // 4. Default assumption: dark background (matches iidy-js)
        true
    }
}
```

#### Enhanced ColorContext with Theme Integration

```rust
// src/output/color.rs

use crate::cli::{ColorChoice, Theme};
use super::theme::OutputTheme;

#[derive(Debug, Clone)]
pub struct ColorContext {
    pub enabled: bool,
    pub theme: OutputTheme,
    pub capabilities: TerminalCapabilities,
}

impl ColorContext {
    pub fn new(color_choice: ColorChoice, theme: Theme) -> Self {
        use std::io::IsTerminal;
        
        let enabled = match color_choice {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                // Respect NO_COLOR environment variable
                if std::env::var("NO_COLOR").is_ok() {
                    false
                } else {
                    std::io::stdout().is_terminal()
                }
            }
        };
        
        let capabilities = TerminalCapabilities::detect();
        let theme = match theme {
            Theme::Auto => OutputTheme::auto(),
            Theme::Dark => OutputTheme::dark(),
            Theme::Light => OutputTheme::light(),
            Theme::HighContrast => OutputTheme::high_contrast(),
        };
        
        Self { enabled, theme, capabilities }
    }
    
    /// Apply theme-aware styling to text
    pub fn style_text(&self, text: &str, role: TextRole) -> String {
        if !self.enabled {
            return text.to_string();
        }
        
        use owo_colors::OwoColorize;
        
        match role {
            TextRole::SectionHeading => {
                if self.capabilities.has_color {
                    text.style(self.theme.section_heading).to_string()
                } else {
                    text.to_string()
                }
            }
            TextRole::PrimaryValue => {
                text.color(self.theme.primary_value).to_string()
            }
            TextRole::SecondaryValue => {
                text.color(self.theme.secondary_value).to_string()
            }
            TextRole::Timestamp => {
                text.color(self.theme.timestamp).to_string()
            }
            TextRole::LogicalId => {
                text.color(self.theme.logical_id).to_string()
            }
            TextRole::MutedText => {
                text.color(self.theme.muted_text).to_string()
            }
            TextRole::ConsoleUrl => {
                text.color(self.theme.console_url).to_string()
            }
            TextRole::Arn => {
                text.color(self.theme.arn).to_string()
            }
            // ... other text roles
        }
    }
    
    /// Colorize CloudFormation status with theme-aware colors
    pub fn colorize_cf_status(&self, status: &str, padding: Option<usize>) -> String {
        if !self.enabled {
            if let Some(width) = padding {
                return format!("{:<width$}", status, width = width);
            } else {
                return status.to_string();
            }
        }
        
        use owo_colors::OwoColorize;
        
        let padded = if let Some(width) = padding {
            format!("{:<width$}", status, width = width)
        } else {
            status.to_string()
        };
        
        // Use exact iidy-js status categorization
        if FAILED.contains(&status) {
            padded.color(self.theme.status_failed).to_string()
        } else if SKIPPED.contains(&status) {
            padded.color(self.theme.status_skipped).to_string()
        } else if COMPLETE.contains(&status) {
            padded.color(self.theme.status_complete).to_string()
        } else if IN_PROGRESS.contains(&status) {
            padded.color(self.theme.status_in_progress).to_string()
        } else {
            padded
        }
    }
    
    /// Apply environment-specific coloring
    pub fn colorize_environment(&self, text: &str, env: &EnvironmentColor) -> String {
        if !self.enabled {
            return text.to_string();
        }
        
        use owo_colors::OwoColorize;
        
        match env {
            EnvironmentColor::Production => text.color(self.theme.env_production).to_string(),
            EnvironmentColor::Integration => text.color(self.theme.env_integration).to_string(),
            EnvironmentColor::Development => text.color(self.theme.env_development).to_string(),
        }
    }
    
    /// Apply token-specific coloring
    pub fn colorize_token(&self, text: &str, token_source: &TokenSource) -> String {
        if !self.enabled {
            return text.to_string();
        }
        
        use owo_colors::OwoColorize;
        
        match token_source {
            TokenSource::UserProvided => text.color(self.theme.token_provided).to_string(),
            TokenSource::AutoGenerated => text.color(self.theme.token_generated).to_string(),
            TokenSource::Derived { .. } => text.color(self.theme.token_derived).to_string(),
        }
    }
}

/// Semantic text roles for theme-aware styling
#[derive(Debug, Clone, Copy)]
pub enum TextRole {
    SectionHeading,
    SectionLabel,
    PrimaryValue,     // Magenta in dark theme
    SecondaryValue,   // Gray text
    Timestamp,        // Light gray
    LogicalId,        // Light gray for resource IDs
    MutedText,        // Very muted gray
    ConsoleUrl,       // URL styling
    Arn,              // ARN styling
    TokenGenerated,   // Generated token styling
    TokenProvided,    // User-provided token styling
    TokenDerived,     // Derived token styling
}

#[derive(Debug, Clone)]
pub struct TerminalCapabilities {
    pub has_color: bool,
    pub has_true_color: bool,
    pub width: Option<usize>,
}

impl TerminalCapabilities {
    pub fn detect() -> Self {
        use std::io::IsTerminal;
        
        let has_color = std::io::stdout().is_terminal() && 
                       std::env::var("NO_COLOR").is_err();
        
        let has_true_color = has_color && 
            std::env::var("COLORTERM")
                .map(|v| v == "truecolor" || v == "24bit")
                .unwrap_or(false);
                
        let width = terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize);
            
        Self { has_color, has_true_color, width }
    }
}

// Status constants (from complete-iidy-implementation-spec.md)
pub const IN_PROGRESS: &[&str] = &[
    "CREATE_IN_PROGRESS", "REVIEW_IN_PROGRESS", "ROLLBACK_IN_PROGRESS",
    "DELETE_IN_PROGRESS", "UPDATE_IN_PROGRESS", "UPDATE_COMPLETE_CLEANUP_IN_PROGRESS",
    "UPDATE_ROLLBACK_IN_PROGRESS", "UPDATE_ROLLBACK_COMPLETE_CLEANUP_IN_PROGRESS",
    "IMPORT_IN_PROGRESS", "IMPORT_ROLLBACK_IN_PROGRESS",
];

pub const COMPLETE: &[&str] = &[
    "CREATE_COMPLETE", "ROLLBACK_COMPLETE", "DELETE_COMPLETE", 
    "UPDATE_COMPLETE", "UPDATE_ROLLBACK_COMPLETE", 
    "IMPORT_COMPLETE", "IMPORT_ROLLBACK_COMPLETE",
];

pub const FAILED: &[&str] = &[
    "CREATE_FAILED", "DELETE_FAILED", "ROLLBACK_FAILED",
    "UPDATE_ROLLBACK_FAILED", "IMPORT_ROLLBACK_FAILED"
];

pub const SKIPPED: &[&str] = &["DELETE_SKIPPED"];
```

#### Themed Interactive Renderer

```rust
// Updated InteractiveRenderer with theme integration

pub struct InteractiveRenderer {
    color_context: ColorContext,
    terminal_width: usize,
    spinner: Option<ProgressManager>,
}

impl InteractiveRenderer {
    pub fn new(color_choice: ColorChoice, theme: Theme) -> Self {
        let color_context = ColorContext::new(color_choice, theme);
        let terminal_width = color_context.capabilities.width.unwrap_or(color_context.theme.default_screen_width);
        
        Self {
            color_context,
            terminal_width,
            spinner: None,
        }
    }
    
    fn format_section_heading(&self, text: &str) -> String {
        self.color_context.style_text(text, TextRole::SectionHeading)
    }
    
    fn format_section_entry(&self, label: &str, value: &str) -> String {
        format!(" {}{}\n",
            self.color_context.style_text(
                &format!("{:<width$} ", label, width = self.color_context.theme.column2_start - 1),
                TextRole::SectionLabel
            ),
            value
        )
    }
    
    fn colorize_resource_status(&self, status: &str, padding: Option<usize>) -> String {
        let effective_padding = padding.unwrap_or(self.color_context.theme.min_status_padding);
        self.color_context.colorize_cf_status(status, Some(effective_padding))
    }
}

#[async_trait]
impl OutputRenderer for InteractiveRenderer {
    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()> {
        println!(); // blank line
        println!("{}", self.format_section_heading("Command Metadata:"));
        
        // Use theme-aware styling instead of hardcoded colors
        print!("{}", self.format_section_entry("CFN Operation:", 
            &self.color_context.style_text(&data.cfn_operation, TextRole::PrimaryValue)));
        print!("{}", self.format_section_entry("iidy Environment:", 
            &self.color_context.style_text(&data.iidy_environment, TextRole::PrimaryValue)));
        print!("{}", self.format_section_entry("Region:", 
            &self.color_context.style_text(&data.region, TextRole::PrimaryValue)));
        
        if let Some(profile) = &data.profile {
            print!("{}", self.format_section_entry("Profile:", 
                &self.color_context.style_text(profile, TextRole::PrimaryValue)));
        }
        
        let cli_args = pretty_format_small_map(&data.cli_arguments);
        print!("{}", self.format_section_entry("CLI Arguments:", 
            &self.color_context.style_text(&cli_args, TextRole::SecondaryValue)));
        
        let service_role = data.iam_service_role.as_deref().unwrap_or("None");
        print!("{}", self.format_section_entry("IAM Service Role:", 
            &self.color_context.style_text(service_role, TextRole::SecondaryValue)));
        
        print!("{}", self.format_section_entry("Current IAM Principal:", 
            &self.color_context.style_text(&data.current_iam_principal, TextRole::SecondaryValue)));
        print!("{}", self.format_section_entry("iidy Version:", 
            &self.color_context.style_text(&data.iidy_version, TextRole::SecondaryValue)));
        
        println!();
        
        // Theme-aware token display
        self.render_token_info(&data.primary_token, &data.derived_tokens);
        
        println!();
        Ok(())
    }
    
    fn render_token_info(&self, primary_token: &TokenInfo, derived_tokens: &[TokenInfo]) {
        // Theme-aware token display with appropriate icons and colors
        match &primary_token.source {
            TokenSource::UserProvided => {
                println!("🔑 Using provided idempotency token {}", 
                    self.color_context.colorize_token(&primary_token.value, &primary_token.source));
            },
            TokenSource::AutoGenerated => {
                println!("🎲 Generated idempotency token {} (save this for retries)", 
                    self.color_context.colorize_token(&primary_token.value, &primary_token.source));
            },
            TokenSource::Derived { from, step } => {
                println!("🔗 Using derived token {} (from {} for step {})", 
                    self.color_context.colorize_token(&primary_token.value, &primary_token.source),
                    &from[..8], step);
            },
        }
        
        // Display derived tokens for multi-step operations
        for token in derived_tokens {
            if let TokenSource::Derived { from, step } = &token.source {
                println!("   🔄 Step '{}' token {} (derived from {})", 
                    step, 
                    self.color_context.colorize_token(&token.value, &token.source),
                    &from[..8]);
            }
        }
    }
    
    async fn render_stack_definition(&mut self, data: &StackDefinition, show_times: bool) -> Result<()> {
        println!("{}", self.format_section_heading("Stack Details:"));
        
        // Name (handle StackSet)
        if let Some(stackset_name) = &data.stackset_name {
            print!("{}", self.format_section_entry("Name (StackSet):", 
                &format!("{} {}", 
                    self.color_context.style_text(&data.name, TextRole::SecondaryValue),
                    self.color_context.style_text(stackset_name, TextRole::PrimaryValue)
                )
            ));
        } else {
            print!("{}", self.format_section_entry("Name:", 
                &self.color_context.style_text(&data.name, TextRole::PrimaryValue)));
        }
        
        // Description with theme-aware coloring
        if let Some(description) = &data.description {
            let desc_styled = if data.name.starts_with("StackSet") {
                self.color_context.style_text(description, TextRole::PrimaryValue)
            } else {
                self.color_context.style_text(description, TextRole::SecondaryValue)
            };
            print!("{}", self.format_section_entry("Description:", &desc_styled));
        }
        
        // Status with theme-aware colorization
        print!("{}", self.format_section_entry("Status", 
            &self.colorize_resource_status(&data.status, None)));
        
        // All other fields using theme-aware styling instead of hardcoded .truecolor(128, 128, 128)
        let capabilities = if data.capabilities.is_empty() {
            "None".to_string()
        } else {
            data.capabilities.join(", ")
        };
        print!("{}", self.format_section_entry("Capabilities:", 
            &self.color_context.style_text(&capabilities, TextRole::SecondaryValue)));
        
        let service_role = data.service_role.as_deref().unwrap_or("None");
        print!("{}", self.format_section_entry("Service Role:", 
            &self.color_context.style_text(service_role, TextRole::SecondaryValue)));
        
        // Continue with all other fields using theme instead of hardcoded colors...
        
        Ok(())
    }
    
    async fn render_stack_list(&mut self, data: &StackListDisplay) -> Result<()> {
        // Header with theme styling
        println!("{}", self.color_context.style_text(&data.header, TextRole::SecondaryValue));
        
        for stack_entry in &data.stacks {
            let time_padding = self.color_context.theme.column2_start; // Use theme padding
            let status_padding = self.color_context.theme.default_status_padding;
            
            // Environment-aware coloring using theme
            let stack_name = if let Some(env_color) = &stack_entry.environment_color {
                self.color_context.colorize_environment(&stack_entry.name, env_color)
            } else {
                stack_entry.name.clone()
            };
            
            // Lifecycle icons with theme styling
            let lifecycle_icon = if let Some(icon) = &stack_entry.lifecycle_icon {
                self.color_context.style_text(icon, TextRole::SecondaryValue)
            } else {
                String::new()
            };
            
            // Main line output with theme-aware timestamp and status
            println!("{} {} {} {}",
                self.color_context.style_text(
                    &format!("{:>width$}", 
                        render_timestamp(stack_entry.timestamp), 
                        width = time_padding
                    ), 
                    TextRole::Timestamp
                ),
                self.colorize_resource_status(&stack_entry.status, Some(status_padding)),
                format!("{}{}", lifecycle_icon, stack_name),
                if !stack_entry.tags.is_empty() { 
                    self.color_context.style_text(
                        &pretty_format_tags(&stack_entry.tags), 
                        TextRole::SecondaryValue
                    )
                } else { 
                    String::new() 
                }
            );
            
            // Failure reason with theme styling
            if let Some(reason) = &stack_entry.failure_reason {
                println!("   {}", 
                    self.color_context.style_text(reason, TextRole::SecondaryValue));
            }
        }
        
        Ok(())
    }
    
    async fn render_command_result(&mut self, data: &CommandResult) -> Result<()> {
        let label = self.format_section_heading(
            &format!("{:<width$}", "Command Summary:", width = self.color_context.theme.column2_start)
        );
        
        if data.success {
            println!("{} {} 👍", label,
                "Success".on_color(self.color_context.theme.success_bg));
        } else {
            println!("{} {} (╯°□°）╯︵ ┻━┻ Fix and try again.", label,
                "Failure".on_color(self.color_context.theme.failure_bg));
        }
        Ok(())
    }
    
    // ... other render methods updated to use theme instead of hardcoded colors
}
```

#### CLI Integration Updates

```rust
// src/cli.rs - Add Theme enum
#[derive(Debug, Args)]
pub struct GlobalOpts {
    #[arg(long, value_enum, global = true, default_value_t = OutputMode::default_for_environment())]
    pub output: OutputMode,
    
    #[arg(long, value_enum, global = true, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
    
    #[arg(long, value_enum, global = true, default_value_t = Theme::Auto)]
    pub theme: Theme,
    
    // ... existing fields
}

impl OutputMode {
    pub fn create_renderer(&self, color_choice: ColorChoice, theme: Theme) -> Box<dyn OutputRenderer> {
        match self {
            OutputMode::Plain => Box::new(PlainTextRenderer::new()), // No colors regardless of theme
            OutputMode::Interactive => Box::new(InteractiveRenderer::new(color_choice, theme)),
            OutputMode::Json => Box::new(JsonRenderer::new()), // No colors
            OutputMode::Tui => Box::new(TuiRenderer::new(color_choice, theme)),
        }
    }
}
```

### Theme Benefits

1. **Exact iidy-js Compatibility**: Dark theme provides pixel-perfect matching using exact xterm color codes
2. **Light Terminal Support**: Light theme adapts colors for readability on light backgrounds  
3. **Accessibility**: High contrast theme ensures visibility for users with visual impairments
4. **Auto-Detection**: Smart detection of terminal background for seamless experience
5. **Semantic Targeting**: CloudFormation-specific color roles rather than generic success/error
6. **Layout Flexibility**: Theme-specific padding and layout constants
7. **Token Integration**: Theme-aware token display matching the token management system
8. **Environment Awareness**: Production/integration/development color coding

### Implementation Strategy

**Phase 1**: Create theme infrastructure and Dark theme (exact iidy-js compatibility)  
**Phase 2**: Implement Light and HighContrast themes with detection logic  
**Phase 3**: Update all renderers to use themes instead of hardcoded colors  
**Phase 4**: Add fixture testing with theme variations  
**Phase 5**: Performance optimization and terminal capability detection

This theme system maintains the exact iidy-js output while enabling modern customization for different terminal environments and accessibility needs.

---

## Control Flow and Data Mapping Analysis

### Command Handler Control Flow

Based on iidy-js `AbstractCloudFormationStackCommand`, the control flow follows this pattern:

```rust
// Main command entry point
async fn create_stack_main(args: &Args) -> Result<i32> {
    // 1. Initialize output manager
    let mut output = DynamicOutputManager::new(args.output_mode, args.output_options()).await?;
    
    // 2. Setup phase - start async operations early
    let cfn_client = CloudFormationClient::new(&aws_config).await;
    let previous_events_future = if show_previous_events {
        Some(get_all_stack_events(&cfn_client, &stack_name))
    } else {
        None
    };
    
    // 3. Command metadata (immediate)
    let metadata = collect_command_metadata(args).await?;
    output.render(OutputData::CommandMetadata(metadata)).await?;
    
    // 4. Get reliable start time for event timing
    let start_time = get_reliable_start_time().await;
    
    // 5. Perform CloudFormation operation
    let stack_id = perform_create_stack(&cfn_client, &stack_args).await?;
    
    // 6. Watch and summarize phase
    watch_and_summarize(&mut output, &cfn_client, &stack_id, start_time, previous_events_future).await
}

async fn watch_and_summarize(
    output: &mut DynamicOutputManager,
    cfn_client: &CloudFormationClient,
    stack_id: &str,
    start_time: DateTime<Utc>,
    previous_events_future: Option<impl Future<Output = Result<Vec<StackEvent>>>>
) -> Result<i32> {
    // 6a. Stack definition (parallel with previous events if available)
    let stack_def_future = collect_stack_definition(cfn_client, stack_id, true);
    
    let (stack_def, previous_events) = if let Some(prev_events_fut) = previous_events_future {
        tokio::try_join!(stack_def_future, prev_events_fut)?
    } else {
        (stack_def_future.await?, vec![])
    };
    
    output.render(OutputData::StackDefinition(stack_def, true)).await?;
    
    // 6b. Previous events (if enabled)
    if !previous_events.is_empty() {
        let previous_display = StackEventsDisplay {
            title: "Previous Stack Events (max 10):".to_string(),
            events: previous_events.into_iter()
                .take(10)
                .map(|e| StackEventWithTiming { event: e, duration_seconds: None })
                .collect(),
            truncated: None, // Set if we truncated
        };
        output.render(OutputData::StackEvents(previous_display)).await?;
    }
    
    // 6c. Live event watching
    let watcher = StackEventWatcher::new(cfn_client, stack_id, start_time);
    watcher.watch_with_output(output).await?;
    
    // 6d. Final stack contents
    let contents = collect_stack_contents(cfn_client, stack_id).await?;
    output.render(OutputData::StackContents(contents)).await?;
    
    // 6e. Success/failure summary
    let success = is_expected_final_status(&contents.current_status.status);
    let result = CommandResult {
        success,
        message: None,
        elapsed_seconds: (Utc::now() - start_time).num_seconds() as u64,
    };
    output.render(OutputData::CommandResult(result)).await?;
    
    Ok(if success { 0 } else { 1 })
}
```

### Stack Event Watcher Integration

The event watcher needs to integrate cleanly with the output manager:

```rust
pub struct StackEventWatcher {
    cfn_client: CloudFormationClient,
    stack_id: String,
    start_time: DateTime<Utc>,
    last_seen_event_time: Option<DateTime<Utc>>,
    seen_events: HashSet<String>, // Event IDs to avoid duplicates
}

impl StackEventWatcher {
    pub async fn watch_with_output(&mut self, output: &mut DynamicOutputManager) -> Result<()> {
        // Initial status message
        let status = StatusUpdate {
            message: "Watching for stack events...".to_string(),
            timestamp: Utc::now(),
        };
        output.render(OutputData::StatusUpdate(status)).await?;
        
        loop {
            // Poll for new events every 2 seconds (matching iidy-js)
            tokio::time::sleep(Duration::from_secs(2)).await;
            
            // Get latest events
            let new_events = self.fetch_new_events().await?;
            
            if !new_events.is_empty() {
                let events_display = StackEventsDisplay {
                    title: "Live Stack Events (2s poll):".to_string(),
                    events: new_events.into_iter()
                        .map(|e| self.add_timing_info(e))
                        .collect(),
                    truncated: None,
                };
                output.render(OutputData::StackEvents(events_display)).await?;
            }
            
            // Check if stack operation is complete
            let stack_status = self.get_current_stack_status().await?;
            if self.is_final_status(&stack_status) {
                break;
            }
            
            // Update spinner with elapsed time (if in interactive mode)
            let elapsed = (Utc::now() - self.start_time).num_seconds();
            let spinner_update = StatusUpdate {
                message: format!("Elapsed time: {}s", elapsed),
                timestamp: Utc::now(),
            };
            output.render(OutputData::StatusUpdate(spinner_update)).await?;
        }
        
        Ok(())
    }
    
    async fn fetch_new_events(&mut self) -> Result<Vec<StackEvent>> {
        let response = self.cfn_client
            .describe_stack_events()
            .stack_name(&self.stack_id)
            .send()
            .await?;
        
        let all_events = response.stack_events().unwrap_or_default();
        
        // Filter to only events after start time and not seen before
        let new_events: Vec<StackEvent> = all_events.iter()
            .filter(|event| {
                // Convert AWS SDK event to our StackEvent type
                let event_time = event.timestamp().map(|ts| DateTime::from(*ts));
                let event_id = event.event_id().unwrap_or_default();
                
                // Only include if after start time and not seen
                event_time.map_or(false, |t| t >= self.start_time) &&
                    !self.seen_events.contains(event_id)
            })
            .map(|aws_event| self.convert_aws_event_to_our_type(aws_event))
            .collect();
        
        // Mark events as seen
        for event in &new_events {
            self.seen_events.insert(event.event_id.clone());
            if let Some(event_time) = event.timestamp {
                if self.last_seen_event_time.map_or(true, |last| event_time > last) {
                    self.last_seen_event_time = Some(event_time);
                }
            }
        }
        
        Ok(new_events)
    }
    
    fn add_timing_info(&self, event: StackEvent) -> StackEventWithTiming {
        let duration_seconds = if let Some(event_time) = event.timestamp {
            Some((Utc::now() - event_time).num_seconds() as u64)
        } else {
            None
        };
        
        StackEventWithTiming {
            event,
            duration_seconds,
        }
    }
}
```

### AWS SDK to OutputData Mapping

Create dedicated collector functions that handle the AWS SDK → struct conversion:

```rust
// src/output/collectors.rs

use aws_sdk_cloudformation as cfn;
use aws_sdk_sts as sts;

pub async fn collect_command_metadata(args: &Args, context: &CfnContext) -> Result<CommandMetadata> {
    let sts_client = sts::Client::new(&aws_config);
    let caller_identity = sts_client.get_caller_identity().send().await?;
    
    // Get derived tokens that have been used so far
    let derived_tokens = context.get_used_tokens();
    
    Ok(CommandMetadata {
        cfn_operation: args.command.to_string(),
        iidy_environment: args.environment.clone(),
        region: args.region.clone(),
        profile: args.profile.clone(),
        cli_arguments: extract_cli_arguments(args),
        iam_service_role: args.stack_args.service_role_arn.clone(),
        current_iam_principal: caller_identity.arn().unwrap_or_default().to_string(),
        iidy_version: env!("CARGO_PKG_VERSION").to_string(),
        primary_token: context.token_info.clone(),
        derived_tokens,
    })
}

fn extract_cli_arguments(args: &Args) -> HashMap<String, String> {
    // Extract relevant CLI arguments for display
    // Based on iidy-js AbstractCloudFormationStackCommand._showCommandSummary()
    let mut cli_args = HashMap::new();
    
    if let Some(region) = &args.region {
        cli_args.insert("region".to_string(), region.clone());
    }
    if let Some(profile) = &args.profile {
        cli_args.insert("profile".to_string(), profile.clone());
    }
    cli_args.insert("argsfile".to_string(), args.argsfile.clone());
    
    cli_args
}

pub async fn collect_stack_definition(
    cfn_client: &cfn::Client,
    stack_id: &str,
    show_times: bool
) -> Result<StackDefinition> {
    let response = cfn_client
        .describe_stacks()
        .stack_name(stack_id)
        .send()
        .await?;
    
    let stack = response.stacks()
        .and_then(|stacks| stacks.first())
        .ok_or_else(|| anyhow::anyhow!("Stack not found"))?;
    
    Ok(StackDefinition {
        name: stack.stack_name().unwrap_or_default().to_string(),
        stackset_name: extract_stackset_name(stack),
        description: stack.description().map(|s| s.to_string()),
        status: stack.stack_status().map(|s| s.as_str()).unwrap_or_default().to_string(),
        capabilities: stack.capabilities()
            .unwrap_or_default()
            .iter()
            .map(|c| c.as_str().to_string())
            .collect(),
        service_role: stack.role_arn().map(|s| s.to_string()),
        tags: extract_tags_as_map(stack.tags()),
        parameters: extract_parameters_as_map(stack.parameters()),
        disable_rollback: stack.disable_rollback().unwrap_or(false),
        termination_protection: stack.enable_termination_protection().unwrap_or(false),
        creation_time: stack.creation_time().map(|ts| DateTime::from(*ts)),
        last_updated_time: stack.last_updated_time().map(|ts| DateTime::from(*ts)),
        timeout_in_minutes: stack.timeout_in_minutes(),
        notification_arns: stack.notification_ar_ns()
            .unwrap_or_default()
            .iter()
            .map(|s| s.to_string())
            .collect(),
        stack_policy: None, // Would need separate API call
        arn: stack.stack_id().unwrap_or_default().to_string(),
        console_url: generate_console_url(stack.stack_id().unwrap_or_default(), &region),
        region: region.clone(),
    })
}

pub async fn collect_stack_contents(
    cfn_client: &cfn::Client,
    stack_id: &str
) -> Result<StackContents> {
    // Parallel fetch of different data
    let (resources_resp, stack_resp) = tokio::try_join!(
        cfn_client.describe_stack_resources().stack_name(stack_id).send(),
        cfn_client.describe_stacks().stack_name(stack_id).send()
    )?;
    
    let stack = stack_resp.stacks()
        .and_then(|stacks| stacks.first())
        .ok_or_else(|| anyhow::anyhow!("Stack not found"))?;
    
    let resources = resources_resp.stack_resources()
        .unwrap_or_default()
        .iter()
        .map(|r| StackResourceInfo {
            logical_id: r.logical_resource_id().unwrap_or_default().to_string(),
            resource_type: r.resource_type().unwrap_or_default().to_string(),
            physical_id: r.physical_resource_id().unwrap_or_default().to_string(),
        })
        .collect();
    
    let outputs = stack.outputs()
        .unwrap_or_default()
        .iter()
        .map(|o| StackOutputInfo {
            key: o.output_key().unwrap_or_default().to_string(),
            value: o.output_value().unwrap_or_default().to_string(),
        })
        .collect();
    
    // Exports would need separate API call to list_exports
    let exports = collect_stack_exports(cfn_client, stack_id).await?;
    
    let current_status = StackStatusInfo {
        status: stack.stack_status().map(|s| s.as_str()).unwrap_or_default().to_string(),
        reason: stack.stack_status_reason().map(|s| s.to_string()),
    };
    
    // Pending changesets would need separate API call
    let pending_changesets = collect_pending_changesets(cfn_client, stack_id).await?;
    
    Ok(StackContents {
        resources,
        outputs,
        exports,
        current_status,
        pending_changesets,
    })
}

// Helper functions for data conversion
fn extract_tags_as_map(tags: Option<&[cfn::types::Tag]>) -> HashMap<String, String> {
    tags.unwrap_or_default()
        .iter()
        .filter_map(|tag| {
            Some((
                tag.key()?.to_string(),
                tag.value()?.to_string()
            ))
        })
        .collect()
}

fn extract_parameters_as_map(params: Option<&[cfn::types::Parameter]>) -> HashMap<String, String> {
    params.unwrap_or_default()
        .iter()
        .filter_map(|param| {
            Some((
                param.parameter_key()?.to_string(),
                param.parameter_value()?.to_string()
            ))
        })
        .collect()
}

fn extract_stackset_name(stack: &cfn::types::Stack) -> Option<String> {
    // Check if this is a StackSet instance
    if stack.stack_name().unwrap_or_default().starts_with("StackSet-") {
        // Extract from tags or description
        extract_tags_as_map(stack.tags()).get("StackSetName").cloned()
            .or_else(|| stack.description().map(|s| s.to_string()))
    } else {
        None
    }
}

fn generate_console_url(stack_id: &str, region: &str) -> String {
    format!("https://{}.console.aws.amazon.com/cloudformation/home?region={}#/stacks/stackinfo?stackId={}", 
        region, region, urlencoding::encode(stack_id))
}

pub async fn collect_stack_exports(cfn_client: &cfn::Client, stack_id: &str) -> Result<Vec<StackExportInfo>> {
    // Get stack exports with their importing stacks
    // This requires the list_exports API call filtered by exporting stack
    let exports_response = cfn_client.list_exports().send().await?;
    
    let mut stack_exports = Vec::new();
    
    if let Some(exports) = exports_response.exports() {
        for export in exports {
            if export.exporting_stack_id() == Some(stack_id) {
                let export_name = export.name().unwrap_or_default();
                
                // Get imports for this export
                let imports_response = cfn_client
                    .list_imports()
                    .export_name(export_name)
                    .send()
                    .await?;
                
                let imported_by = imports_response.imports()
                    .unwrap_or_default()
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                
                stack_exports.push(StackExportInfo {
                    name: export_name.to_string(),
                    value: export.value().unwrap_or_default().to_string(),
                    imported_by,
                });
            }
        }
    }
    
    Ok(stack_exports)
}

pub async fn collect_pending_changesets(cfn_client: &cfn::Client, stack_id: &str) -> Result<Vec<ChangeSetInfo>> {
    let changesets_response = cfn_client
        .list_change_sets()
        .stack_name(stack_id)
        .send()
        .await?;
    
    let mut changesets = Vec::new();
    
    if let Some(summaries) = changesets_response.summaries() {
        for summary in summaries {
            // Get detailed changeset info
            let changeset_details = cfn_client
                .describe_change_set()
                .stack_name(stack_id)
                .change_set_name(summary.change_set_name().unwrap_or_default())
                .send()
                .await?;
            
            let changes = changeset_details.changes()
                .unwrap_or_default()
                .iter()
                .map(|change| {
                    // Convert AWS change to our ChangeInfo structure
                    ChangeInfo {
                        action: change.action().map(|a| a.as_str()).unwrap_or_default().to_string(),
                        logical_resource_id: change.resource_change()
                            .and_then(|rc| rc.logical_resource_id())
                            .unwrap_or_default()
                            .to_string(),
                        resource_type: change.resource_change()
                            .and_then(|rc| rc.resource_type())
                            .unwrap_or_default()
                            .to_string(),
                        replacement: change.resource_change()
                            .and_then(|rc| rc.replacement())
                            .map(|r| r.as_str().to_string()),
                        details: vec![], // Would need more detailed parsing
                    }
                })
                .collect();
            
            changesets.push(ChangeSetInfo {
                name: summary.change_set_name().unwrap_or_default().to_string(),
                status: summary.status().map(|s| s.as_str()).unwrap_or_default().to_string(),
                status_reason: summary.status_reason().map(|s| s.to_string()),
                description: summary.description().map(|s| s.to_string()),
                creation_time: summary.creation_time()
                    .map(|ts| DateTime::from(*ts))
                    .unwrap_or_else(|| Utc::now()),
                changes,
            });
        }
    }
    
    Ok(changesets)
}

pub async fn collect_stack_events(
    cfn_client: &cfn::Client,
    stack_id: &str,
    limit: usize,
    title: &str
) -> Result<StackEventsDisplay> {
    let response = cfn_client
        .describe_stack_events()
        .stack_name(stack_id)
        .send()
        .await?;
    
    let all_events = response.stack_events().unwrap_or_default();
    let total_events = all_events.len();
    
    // Convert AWS events to our StackEvent type and take the requested limit
    let events: Vec<StackEventWithTiming> = all_events
        .iter()
        .take(limit)
        .map(|aws_event| {
            let stack_event = StackEvent {
                event_id: aws_event.event_id().unwrap_or_default().to_string(),
                stack_id: aws_event.stack_id().unwrap_or_default().to_string(),
                stack_name: aws_event.stack_name().unwrap_or_default().to_string(),
                logical_resource_id: aws_event.logical_resource_id().unwrap_or_default().to_string(),
                physical_resource_id: aws_event.physical_resource_id().map(|s| s.to_string()),
                resource_type: aws_event.resource_type().unwrap_or_default().to_string(),
                timestamp: aws_event.timestamp().map(|ts| DateTime::from(*ts)),
                resource_status: aws_event.resource_status().map(|s| s.as_str()).unwrap_or_default().to_string(),
                resource_status_reason: aws_event.resource_status_reason().map(|s| s.to_string()),
                resource_properties: aws_event.resource_properties().map(|s| s.to_string()),
                client_request_token: aws_event.client_request_token().map(|s| s.to_string()),
            };
            
            StackEventWithTiming {
                event: stack_event,
                duration_seconds: None, // Will be calculated during live watching
            }
        })
        .collect();
    
    let truncated = if total_events > limit {
        Some(TruncationInfo {
            shown: limit,
            total: total_events,
        })
    } else {
        None
    };
    
    Ok(StackEventsDisplay {
        title: title.to_string(),
        events,
        truncated,
    })
}
```

### Error Handling and Propagation

Errors should flow through the output system:

```rust
impl From<aws_sdk_cloudformation::Error> for ErrorInfo {
    fn from(err: aws_sdk_cloudformation::Error) -> Self {
        ErrorInfo {
            message: err.to_string(),
            context: Some("CloudFormation API".to_string()),
            timestamp: Utc::now(),
        }
    }
}

// In command handlers
async fn create_stack_main(args: &Args) -> Result<i32> {
    let mut output = DynamicOutputManager::new(args.output_mode, args.output_options()).await?;
    
    // ... normal flow
    
    match perform_create_stack(&cfn_client, &stack_args).await {
        Ok(stack_id) => {
            // Continue with normal flow
            watch_and_summarize(&mut output, &cfn_client, &stack_id, start_time, previous_events_future).await
        }
        Err(e) => {
            let error_info = ErrorInfo::from(e);
            output.render(OutputData::Error(error_info)).await?;
            Ok(1) // Exit code 1 for failure
        }
    }
}
```

### Data Flow Summary

1. **Commands** orchestrate the overall flow and handle AWS operations
2. **Collectors** convert AWS SDK types to our display structs  
3. **Event Watcher** continuously polls and feeds new events to output manager
4. **Output Manager** handles rendering and mode switching
5. **Renderers** format data appropriately for each output mode

This separation ensures:
- Commands focus on AWS operations and orchestration
- Data collection is centralized and reusable
- Event watching integrates seamlessly with output modes
- Error handling flows through the same rendering pipeline
- Mode switching works at any point during execution

---

## Offline Testing and Fixture-Based Integration

### Requirements for Offline Testing

Based on the token management design, we need **fully offline, deterministic testing** for:

1. **Output Rendering**: All four output modes (Interactive, Plain, JSON, TUI)
2. **Dynamic Mode Switching**: Event replay and mode transitions
3. **Multi-Step Operations**: Token derivation and step coordination
4. **Error Handling**: Graceful failure scenarios
5. **Edge Cases**: Empty data, malformed responses, network timeouts

### Fixture-Based Testing Architecture

```rust
// Extend OutputData to support fixture loading
#[derive(Clone, Debug)]
pub enum OutputData {
    CommandMetadata(CommandMetadata),
    StackDefinition(StackDefinition, bool),
    StackEvents(StackEventsDisplay),
    StackContents(StackContents),
    StatusUpdate(StatusUpdate),
    CommandResult(CommandResult),
    StackList(StackListDisplay),
    ChangeSetResult(ChangeSetCreationResult),
    Error(ErrorInfo),
    
    // Test-only variants for fixture loading
    #[cfg(test)]
    FixtureLoaded(FixtureData),
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub struct FixtureData {
    pub name: String,
    pub description: String,
    pub expected_output: HashMap<OutputMode, String>,
}
```

### Test Fixture Structure

Fixtures capture deterministic AWS responses and expected output for each mode:

```rust
// tests/fixtures/mod.rs
pub struct TestFixture {
    pub name: String,
    pub description: String,
    pub aws_responses: AwsResponseSet,
    pub expected_outputs: HashMap<OutputMode, ExpectedOutput>,
    pub tokens: FixtureTokens,
}

pub struct AwsResponseSet {
    pub create_stack: Option<aws_sdk_cloudformation::output::CreateStackOutput>,
    pub describe_stacks: Option<aws_sdk_cloudformation::output::DescribeStacksOutput>, 
    pub describe_stack_events: Option<aws_sdk_cloudformation::output::DescribeStackEventsOutput>,
    pub describe_stack_resources: Option<aws_sdk_cloudformation::output::DescribeStackResourcesOutput>,
    // ... other AWS responses
}

pub struct ExpectedOutput {
    pub stdout: String,      // Exact expected output
    pub stderr: String,      // Expected error output
    pub exit_code: i32,      // Expected exit code
    pub line_count: usize,   // For basic validation
}

pub struct FixtureTokens {
    pub primary: String,                    // Fixed primary token for deterministic testing
    pub derived: HashMap<String, String>,   // Step name -> expected derived token
}
```

### Fixture Files and Test Data

```yaml
# tests/fixtures/create-stack-happy-path.yaml
name: "create-stack-happy-path"
description: "Successful stack creation with all standard sections"

tokens:
  primary: "test-token-12345678-abcd-efgh-ijkl-123456789012"
  derived:
    create-stack: "test-token-a1b2c3d4"

aws_responses:
  describe_stacks:
    stacks:
      - stack_name: "test-stack"
        stack_status: "CREATE_COMPLETE"
        creation_time: "2024-01-15T10:30:00Z"
        # ... complete mock stack response
        
  describe_stack_events:
    stack_events:
      - event_id: "event-001"
        stack_name: "test-stack"
        logical_resource_id: "test-stack"
        resource_type: "AWS::CloudFormation::Stack"
        resource_status: "CREATE_COMPLETE"
        timestamp: "2024-01-15T10:30:00Z"
        # ... complete mock events

expected_outputs:
  interactive:
    stdout: |
      
      Command Metadata:
       CFN Operation:        create-stack
       iidy Environment:     test
       Region:               us-east-1
       CLI Arguments:        {argsfile: stack-args.yaml}
       IAM Service Role:     None
       Current IAM Principal: arn:aws:iam::123456789012:user/test-user
       iidy Version:         0.1.0
       
      🎲 Generated idempotency token test-token-12345678-abcd-efgh-ijkl-123456789012 (save this for retries)
      
      Stack Details:
       Name:                 test-stack
       Status:               CREATE_COMPLETE
       # ... rest of expected interactive output
    exit_code: 0
    
  plain:
    stdout: |
      Command Metadata:
       CFN Operation:        create-stack
       iidy Environment:     test
       Region:               us-east-1
       CLI Arguments:        {argsfile: stack-args.yaml}
       # ... plain text version (no colors, no emojis)
    exit_code: 0
    
  json:
    stdout: |
      {"type":"command_metadata","timestamp":"2024-01-15T10:30:00Z","data":{"cfn_operation":"create-stack",...}}
      {"type":"stack_definition","timestamp":"2024-01-15T10:30:00Z","data":{"name":"test-stack",...}}
      {"type":"command_result","timestamp":"2024-01-15T10:30:00Z","data":{"success":true,...}}
    exit_code: 0
```

### Test Implementation

```rust
// tests/integration/output_modes.rs
use crate::fixtures::TestFixture;

#[tokio::test]
async fn test_create_stack_happy_path_all_modes() {
    let fixture = TestFixture::load("create-stack-happy-path").await.unwrap();
    
    for (mode, expected) in &fixture.expected_outputs {
        // Create deterministic output manager with fixture responses
        let mut output = DynamicOutputManager::new_with_fixture(*mode, &fixture).await.unwrap();
        
        // Execute command with fixture responses
        let exit_code = run_create_stack_with_fixture(&fixture, &mut output).await.unwrap();
        
        // Validate output
        let actual_output = output.get_captured_output();
        assert_eq!(actual_output.trim(), expected.stdout.trim(), 
                   "Output mismatch for mode {:?}", mode);
        assert_eq!(exit_code, expected.exit_code, 
                   "Exit code mismatch for mode {:?}", mode);
    }
}

#[tokio::test] 
async fn test_dynamic_mode_switching_with_replay() {
    let fixture = TestFixture::load("update-stack-changeset").await.unwrap();
    let mut output = DynamicOutputManager::new(OutputMode::Interactive, 
                                               OutputOptions::default()).await.unwrap();
    
    // Send some data in Interactive mode
    output.render(OutputData::CommandMetadata(fixture.command_metadata.clone())).await.unwrap();
    output.render(OutputData::StackDefinition(fixture.stack_def.clone(), true)).await.unwrap();
    
    // Switch to JSON mode - should replay all previous data
    output.switch_to_mode(OutputMode::Json).await.unwrap();
    
    // Send new data in JSON mode  
    output.render(OutputData::CommandResult(fixture.command_result.clone())).await.unwrap();
    
    // Validate that replay worked correctly
    let captured = output.get_captured_output();
    assert!(captured.contains("\"type\":\"command_metadata\""));
    assert!(captured.contains("\"type\":\"stack_definition\""));
    assert!(captured.contains("\"type\":\"command_result\""));
}

#[tokio::test]
async fn test_token_display_deterministic() {
    let fixture = TestFixture::load("multi-step-changeset").await.unwrap();
    let mut output = DynamicOutputManager::new_with_fixture(OutputMode::Interactive, &fixture).await.unwrap();
    
    // Run multi-step operation
    run_update_stack_changeset_with_fixture(&fixture, &mut output).await.unwrap();
    
    let captured = output.get_captured_output();
    
    // Verify primary token display
    assert!(captured.contains(&format!("🎲 Generated idempotency token {}", fixture.tokens.primary)));
    
    // Verify derived tokens display  
    for (step, expected_token) in &fixture.tokens.derived {
        assert!(captured.contains(&format!("🔄 Step '{}' token {}", step, expected_token)));
    }
}
```

### Fixture-Based AWS Client Mock

```rust
// src/output/testing.rs (new module)
#[cfg(test)]
pub struct FixtureAwsClient {
    fixture: TestFixture,
    call_count: Arc<Mutex<HashMap<String, usize>>>,
}

#[cfg(test)]
impl FixtureAwsClient {
    pub fn new(fixture: TestFixture) -> Self {
        Self {
            fixture,
            call_count: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    fn track_call(&self, operation: &str) {
        if let Ok(mut calls) = self.call_count.lock() {
            *calls.entry(operation.to_string()).or_insert(0) += 1;
        }
    }
}

#[cfg(test)]
#[async_trait]
impl CloudFormationApi for FixtureAwsClient {
    async fn describe_stacks(&self, _input: DescribeStacksInput) -> Result<DescribeStacksOutput> {
        self.track_call("describe_stacks");
        
        Ok(self.fixture.aws_responses.describe_stacks
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No describe_stacks fixture response"))?)
    }
    
    async fn create_stack(&self, input: CreateStackInput) -> Result<CreateStackOutput> {
        self.track_call("create_stack");
        
        // Validate expected token was used
        assert_eq!(input.client_request_token(), Some(&self.fixture.tokens.primary));
        
        Ok(self.fixture.aws_responses.create_stack
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No create_stack fixture response"))?)
    }
    
    // ... other AWS operations with fixture responses
}
```

### Running Fixture Tests

```bash
# Run all fixture-based tests
cargo test --test output_integration

# Run specific fixture test
cargo test test_create_stack_happy_path_all_modes

# Run with fixture validation (checks fixture file format)
cargo test --features validate-fixtures

# Generate new fixture from live AWS (for reference/debugging)
cargo test --features generate-fixtures --ignored generate_create_stack_fixture
```

### Benefits of Fixture-Based Testing

1. **🔄 Deterministic**: Same fixture always produces same output across all modes
2. **⚡ Fast**: No network latency, tests run in milliseconds
3. **🔒 Offline**: Complete isolation from AWS, works without internet
4. **🎯 Comprehensive**: Test all output modes, edge cases, and error scenarios
5. **🧪 Reproducible**: CI/CD gets identical results every time
6. **📋 Traceable**: Token derivation and multi-step flows fully testable
7. **🛡️ Safe**: No risk of accidentally modifying real AWS resources
8. **📚 Documented**: Fixtures serve as examples of expected behavior

This fixture-based approach enables comprehensive testing of the entire output system while maintaining the deterministic, offline requirements from the token management design.

---

## Module Structure and Directory Layout

### Proposed Directory Structure

```
src/
├── output/                    # New: Complete output system
│   ├── mod.rs                # Public API exports
│   ├── data.rs               # OutputData enum and display structs (includes TokenInfo)
│   ├── collectors.rs         # AWS SDK → display struct conversion (includes token integration)
│   ├── manager.rs            # DynamicOutputManager with mode switching and replay
│   ├── watcher.rs            # StackEventWatcher for live CloudFormation events
│   ├── testing.rs            # Fixture-based testing infrastructure (test-only)
│   ├── renderers/            # Output mode implementations
│   │   ├── mod.rs
│   │   ├── interactive.rs    # InteractiveRenderer (exact iidy-js match + token display)
│   │   ├── plain.rs          # PlainTextRenderer (CI-friendly, no colors)
│   │   ├── json.rs           # JsonRenderer (machine-readable JSONL)
│   │   └── tui.rs            # TuiRenderer (full-screen interface)
│   └── keyboard.rs           # Non-blocking keyboard input for mode switching
├── tests/                     # Enhanced test structure
│   ├── fixtures/             # Test fixture definitions
│   │   ├── mod.rs            # Fixture loading and validation
│   │   ├── create-stack-happy-path.yaml
│   │   ├── update-stack-changeset.yaml
│   │   ├── multi-step-operations.yaml
│   │   ├── error-scenarios.yaml
│   │   └── edge-cases.yaml
│   ├── integration/          # Integration tests
│   │   ├── output_modes.rs   # Test all output modes with fixtures
│   │   ├── mode_switching.rs # Test dynamic mode switching and replay
│   │   ├── token_integration.rs # Test token display and derivation
│   │   └── error_handling.rs # Test error scenarios across modes
│   └── snapshots/            # Expected output snapshots (for validation)
├── cfn/                      # Existing: CloudFormation operations
│   ├── mod.rs
│   ├── create_stack.rs       # Modified to use output system
│   ├── update_stack.rs       # Modified to use output system
│   ├── describe_stack.rs     # Modified to use output system
│   ├── list_stacks.rs        # Modified to use output system
│   ├── watch_stack.rs        # Deprecated - replaced by output/watcher.rs
│   └── ...                   # Other existing CFN operations
├── cli.rs                    # Modified: Add --output flag and OutputMode enum
├── aws.rs                    # Existing: AWS client configuration
├── stack_args.rs             # Existing: Stack configuration parsing
├── color.rs                  # Existing: Color utilities (used by renderers)
├── terminal.rs               # Existing: Terminal utilities (used by renderers)
├── timing.rs                 # Existing: Time utilities (used by collectors)
└── ...                       # Other existing modules
```

### Integration with Existing Modules

**Enhanced CLI Module** (`src/cli.rs`):
```rust
// Add to existing GlobalOpts
#[derive(Debug, Args)]
pub struct GlobalOpts {
    #[arg(long, value_enum, global = true, default_value_t = OutputMode::default_for_environment())]
    pub output: OutputMode,
    
    #[arg(long, value_enum, global = true, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
    
    // ... existing fields
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputMode {
    /// Non-interactive text for CI/logs
    Plain,
    /// Interactive text with exact iidy-js formatting
    Interactive, 
    /// Machine-readable JSON Lines
    Json,
    /// Terminal User Interface
    Tui,
}
```

**Modified CFN Commands** (e.g., `src/cfn/create_stack.rs`):
```rust
use crate::output::{DynamicOutputManager, OutputData, collectors};

pub async fn create_stack_main(args: &Args) -> Result<i32> {
    // Replace all println!/display logic with output manager
    let mut output = DynamicOutputManager::new(args.output, args.output_options()).await?;
    
    // Use collectors for data conversion
    let metadata = collectors::collect_command_metadata(args).await?;
    output.render(OutputData::CommandMetadata(metadata)).await?;
    
    // ... rest of implementation using output system
}
```

### New Output Module Structure

**Main Module** (`src/output/mod.rs`):
```rust
//! Multi-mode output system for iidy commands
//! 
//! Provides exact iidy-js compatibility in Interactive mode,
//! CI-friendly Plain mode, machine-readable JSON mode,
//! and full-screen TUI mode with dynamic switching.

pub use data::*;
pub use manager::DynamicOutputManager;
pub use collectors::*;
pub use renderers::OutputRenderer;
pub use watcher::StackEventWatcher;

mod data;
mod collectors; 
mod manager;
mod watcher;
mod keyboard;
pub mod renderers;

// Re-export commonly used types
pub use chrono::{DateTime, Utc};
pub use std::collections::HashMap;
```

**Data Structures** (`src/output/data.rs`):
```rust
//! Display data structures that capture exactly what iidy-js shows
//! 
//! These structs separate data collection from presentation,
//! enabling multiple output modes to format the same data differently.

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

// All the structs we defined: CommandMetadata, StackDefinition, etc.
// Plus the OutputData enum for dynamic dispatch
```

**Collectors Module** (`src/output/collectors.rs`):
```rust
//! AWS SDK data collection and conversion to display structs
//! 
//! Handles all the complex mapping from AWS CloudFormation SDK types
//! to our clean display data structures.

use aws_sdk_cloudformation as cfn;
use aws_sdk_sts as sts;
use anyhow::Result;
use super::data::*;

// All the collect_* functions we defined
```

**Manager Module** (`src/output/manager.rs`):
```rust
//! Dynamic output manager with mode switching and event buffering
//! 
//! Orchestrates rendering across different output modes and enables
//! real-time mode switching with full event history replay.

use super::{OutputData, OutputRenderer, keyboard::KeyboardListener};
use anyhow::Result;
use tokio::sync::mpsc;

pub struct DynamicOutputManager {
    // Implementation as defined
}
```

**Event Watcher** (`src/output/watcher.rs`):
```rust
//! CloudFormation stack event monitoring and live updates
//! 
//! Continuously polls for new stack events and integrates with
//! the output manager for real-time display updates.

use aws_sdk_cloudformation as cfn;
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use super::{OutputData, DynamicOutputManager, data::*};

pub struct StackEventWatcher {
    // Implementation as defined
}
```

**Interactive Renderer** (`src/output/renderers/interactive.rs`):
```rust
//! Interactive renderer with exact iidy-js output matching
//! 
//! Implements pixel-perfect compatibility with original iidy-js,
//! including all colors, spacing, icons, and formatting.

use super::OutputRenderer;
use crate::color::{ColorContext, UserDefined}; // Reuse existing color utilities
use crate::terminal::get_terminal_width;       // Reuse existing terminal utilities
use async_trait::async_trait;
use owo_colors::OwoColorize;

pub struct InteractiveRenderer {
    // Implementation as defined
}

// Import all the exact formatting constants and functions from implementation spec
const COLUMN2_START: usize = 25;              // From complete-iidy-implementation-spec.md
const DEFAULT_STATUS_PADDING: usize = 35;     // From complete-iidy-implementation-spec.md
const MIN_STATUS_PADDING: usize = 17;         // From complete-iidy-implementation-spec.md
const MAX_PADDING: usize = 60;                // From complete-iidy-implementation-spec.md
const RESOURCE_TYPE_PADDING: usize = 40;      // From complete-iidy-implementation-spec.md
const DEFAULT_SCREEN_WIDTH: usize = 130;      // From complete-iidy-implementation-spec.md

// Color constants (exact xterm values from implementation spec)
const COLOR_TIMESTAMP: u8 = 253;              // Light gray
const COLOR_LOGICAL_ID: u8 = 252;             // Light gray  
const COLOR_SECTION_HEADING: u8 = 255;        // White
const COLOR_SPINNER: u8 = 240;                // Dark gray
const COLOR_ENV_INTEGRATION: u8 = 75;         // Blue-ish
const COLOR_ENV_DEVELOPMENT: u8 = 194;        // Yellow-ish
```

### Integration Points with Existing Code

**Reuse Existing Utilities**:
- `src/color.rs` - Color context and utilities for terminal color support
- `src/terminal.rs` - Terminal width detection and TTY checking
- `src/timing.rs` - Time formatting and duration calculations
- `src/aws.rs` - AWS client configuration and region handling

**Deprecate/Replace**:
- `src/cfn/watch_stack.rs` - Replace with `src/output/watcher.rs`
- `src/display.rs` - Replace with output renderer system
- `src/render.rs` - Replace with output renderer system

**Enhance Existing**:
- `src/cli.rs` - Add `--output` flag and `OutputMode` enum
- `src/cfn/*.rs` - Update all commands to use `DynamicOutputManager`

### Migration Strategy

**Phase 1: Core Infrastructure**
1. Create `src/output/` directory structure
2. Implement data structures in `data.rs`
3. Create basic `DynamicOutputManager` in `manager.rs`
4. Add `--output` flag to CLI

**Phase 2: Interactive Renderer** 
1. Implement `InteractiveRenderer` with exact iidy-js formatting
2. Port all formatting constants and functions from implementation spec
3. Test against iidy-js output samples for pixel-perfect matching

**Phase 3: Command Integration**
1. Migrate `create_stack.rs` to use output system
2. Migrate `describe_stack.rs` and `list_stacks.rs`
3. Implement collectors for all AWS SDK data conversion

**Phase 4: Additional Modes**
1. Implement `PlainTextRenderer` for CI environments
2. Implement `JsonRenderer` for machine consumption
3. Add keyboard listener for dynamic mode switching

**Phase 5: Advanced Features**
1. Implement `TuiRenderer` with ratatui
2. Add event replay functionality
3. Performance optimization and testing

### Benefits of This Structure

1. **Clear Separation**: Output logic completely separated from AWS operations
2. **Reusability**: Existing color/terminal utilities can be reused
3. **Testability**: Each renderer can be tested independently with fixtures
4. **Maintainability**: Changes to AWS operations don't affect output formatting
5. **Extensibility**: Easy to add new output modes without touching core logic
6. **Backward Compatibility**: Existing CFN modules enhanced, not replaced
7. **Offline Testing**: Complete fixture-based testing with deterministic results
8. **Token Integration**: Seamless integration with existing token management system

---

## Integration Summary: Output System + Token Management

### Unified Architecture Benefits

The integration of our data-driven output architecture with the existing token management system provides:

#### 🔧 **Technical Integration**
- **TokenInfo Display**: Primary and derived tokens shown in Command Metadata section
- **Deterministic Testing**: Fixed tokens in fixtures ensure reproducible output across all modes
- **Multi-Step Coordination**: Output system displays token derivation for changeset operations
- **Error Correlation**: Token information included in error display for debugging

#### 🧪 **Testing Completeness**
- **Offline First**: All tests run without AWS API calls using fixture responses
- **Mode Coverage**: Every output mode tested with identical fixture data
- **Token Validation**: Verify correct token derivation and display formatting
- **Edge Cases**: Test error scenarios, network timeouts, malformed responses

#### 📊 **Output Consistency**
- **Interactive Mode**: Exact iidy-js compatibility including token display format
- **Plain Mode**: Clean CI-friendly output with tokens but no colors/emojis
- **JSON Mode**: Structured token data for machine consumption
- **TUI Mode**: Rich interface with token information in status panels

#### 🔄 **Operational Flow**
```rust
// Unified command flow with token integration
async fn create_stack_main(args: &Args) -> Result<i32> {
    // 1. Create context with token management (existing system)
    let context = create_context(&args.normalized_aws_opts).await?;
    
    // 2. Initialize output manager with fixture support
    let mut output = DynamicOutputManager::new(args.output_mode, args.output_options()).await?;
    
    // 3. Display command metadata with tokens (new integration)
    let metadata = collect_command_metadata(args, &context).await?;
    output.render(OutputData::CommandMetadata(metadata)).await?;
    
    // 4. Execute CloudFormation operations with token derivation
    let start_time = context.get_reliable_start_time().await;
    let stack_id = perform_create_stack(&context, &stack_args).await?;
    
    // 5. Watch events and display results
    watch_and_summarize(&mut output, &context, &stack_id, start_time).await
}
```

#### 🎯 **Key Achievements**

1. **Seamless Integration**: No breaking changes to existing token management
2. **Enhanced Visibility**: Token information now visible in all output modes
3. **Complete Testing**: 100% offline testing with deterministic fixtures
4. **User Experience**: Consistent token display matching established patterns
5. **Developer Experience**: Easy fixture creation and test maintenance
6. **CI/CD Ready**: Fast, reliable tests with no external dependencies

This unified architecture delivers on both the pixel-perfect iidy-js compatibility requirement and the offline, deterministic testing requirement while maintaining the robust token management system already in place.
