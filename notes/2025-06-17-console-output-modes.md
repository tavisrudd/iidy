# Console Output Modes Design

**Date:** 2025-06-17  
**Status:** Design Document  
**Priority:** High

## Overview

This document outlines the design for supporting multiple output modes in iidy's Rust port for CloudFormation stack-related commands. The system provides four distinct output modes optimized for different consumers (humans in CI, interactive humans, machines, and TUI users).

**Related Documents:**
- [`2025-06-17-complete-iidy-implementation-spec.md`](./2025-06-17-complete-iidy-implementation-spec.md) - Complete specification for exact iidy-js output matching
- [`2025-06-17-data-driven-output-architecture.md`](./2025-06-17-data-driven-output-architecture.md) - Data structures and rendering traits

## Architecture Overview

The design uses a **data-driven architecture** that separates data collection from presentation:

1. **Data Structures** capture exactly what iidy-js displays (from implementation spec)
2. **Renderer Traits** handle presentation logic for each output mode
3. **Dynamic Manager** enables real-time mode switching with event replay
4. **Commands** focus on data collection and orchestration

## Output Modes

### 1. Non-Interactive Streaming Text (CI/Logs)
- **Target**: Humans reading logs in CI systems
- **Features**:
  - No spinners or dynamic updates
  - New events appended as lines
  - Color disabled in non-TTY unless `--color=always`
  - Simple, linear output format

### 2. Interactive Streaming Text (Default)
- **Target**: Humans in interactive terminals
- **Features**:
  - **Exact match** of original iidy-js output
  - Spinners with time-since-last-event updates
  - Colors, column formatting per implementation spec
  - Dynamic status updates

### 3. Machine-Readable Streaming (JSONL)
- **Target**: Other tools, LLMs, automation
- **Features**:
  - JSON Lines format (one JSON object per line)
  - Structured data for all events
  - No formatting or colors
  - Parseable output

### 4. Interactive TUI Mode
- **Target**: Humans wanting rich interaction
- **Features**:
  - Full-screen terminal UI
  - Real-time updates
  - Keyboard navigation
  - Enhanced visualization

## Core Architecture

### Data Structures (from data-driven spec)

The system uses precise data structures that capture exactly what iidy-js displays:

```rust
// From 2025-06-17-data-driven-output-architecture.md
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
}

#[derive(Clone, Debug)]
pub struct StackDefinition {
    pub name: String,
    pub stackset_name: Option<String>,
    pub description: Option<String>,
    pub status: String,
    // ... all fields from summarizeStackDefinition
}

#[derive(Clone, Debug)]
pub struct StackEventsDisplay {
    pub title: String, // "Previous Stack Events (max 10):" or "Live Stack Events (2s poll):"
    pub events: Vec<StackEventWithTiming>,
    pub truncated: Option<TruncationInfo>,
}

#[derive(Clone, Debug)]
pub struct StackContents {
    pub resources: Vec<StackResourceInfo>,
    pub outputs: Vec<StackOutputInfo>,
    pub exports: Vec<StackExportInfo>,
    pub current_status: StackStatusInfo,
    pub pending_changesets: Vec<ChangeSetInfo>,
}

// Additional data structures for other display sections...
```

### Renderer Trait

```rust
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

### Mode Implementations

#### Interactive Renderer (Exact iidy-js Match)

```rust
pub struct InteractiveRenderer {
    color_context: ColorContext,
    terminal_width: usize,
    spinner: Option<ProgressManager>,
}

#[async_trait]
impl OutputRenderer for InteractiveRenderer {
    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()> {
        // Exact implementation from complete-iidy-implementation-spec.md
        println!(); // blank line
        println!("{}", format_section_heading("Command Metadata:"));
        
        print_section_entry("CFN Operation:", &data.cfn_operation.magenta().to_string());
        print_section_entry("iidy Environment:", &data.iidy_environment.magenta().to_string());
        print_section_entry("Region:", &data.region.magenta().to_string());
        
        if let Some(profile) = &data.profile {
            print_section_entry("Profile:", &profile.magenta().to_string());
        }
        
        let cli_args = pretty_format_small_map(&data.cli_arguments);
        print_section_entry("CLI Arguments:", &cli_args.truecolor(128, 128, 128).to_string());
        
        // ... exact formatting from implementation spec
        
        println!();
        Ok(())
    }
    
    async fn render_stack_events(&mut self, data: &StackEventsDisplay) -> Result<()> {
        println!("{}", format_section_heading(&data.title));
        
        if data.events.is_empty() {
            return Ok(());
        }
        
        // Use exact displayStackEvent algorithm from implementation spec
        let status_padding = calc_padding(&data.events, |e| &e.event.resource_status);
        
        for event_with_timing in &data.events {
            display_stack_event(&event_with_timing.event, status_padding, event_with_timing.duration_seconds);
        }
        
        // Show truncation message exactly as iidy-js
        if let Some(truncation) = &data.truncated {
            println!("{}", 
                format!(" {} of {} total events shown", truncation.shown, truncation.total)
                    .truecolor(128, 128, 128)
            );
        }
        
        Ok(())
    }
    
    // ... other render methods with exact iidy-js formatting
}
```

#### Plain Text Renderer

```rust
pub struct PlainTextRenderer {
    // No colors, no spinners - CI-friendly
}

#[async_trait]
impl OutputRenderer for PlainTextRenderer {
    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()> {
        println!("Command Metadata:");
        println!(" CFN Operation:        {}", data.cfn_operation);
        println!(" iidy Environment:     {}", data.iidy_environment);
        println!(" Region:               {}", data.region);
        // ... plain text formatting without colors
        Ok(())
    }
    
    // ... plain versions of all render methods
}
```

#### JSON Renderer

```rust
pub struct JsonRenderer;

#[async_trait]  
impl OutputRenderer for JsonRenderer {
    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()> {
        let json_data = json!({
            "type": "command_metadata",
            "timestamp": Utc::now().to_rfc3339(),
            "data": data
        });
        println!("{}", serde_json::to_string(&json_data)?);
        Ok(())
    }
    
    // ... JSON versions of all render methods
}
```

## Dynamic Mode Switching

### Output Data Buffer

```rust
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
```

### Dynamic Manager

```rust
pub struct DynamicOutputManager {
    current_mode: OutputMode,
    current_renderer: Box<dyn OutputRenderer>,
    event_buffer: Vec<OutputData>,
    options: OutputOptions,
    keyboard_listener: Option<KeyboardListener>,
}

impl DynamicOutputManager {
    pub async fn render(&mut self, data: OutputData) -> Result<()> {
        // Buffer the data for mode switching replay
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
            OutputData::StackContents(ref contents) => {
                self.current_renderer.render_stack_contents(contents).await?;
            }
            // ... other cases
        }
        
        // Check for mode switch (non-blocking)
        self.check_mode_switch().await?;
        
        Ok(())
    }
    
    async fn switch_to_mode(&mut self, new_mode: OutputMode) -> Result<()> {
        // Clean up current renderer
        self.current_renderer.cleanup().await?;
        
        // Clear screen if switching to/from TUI
        if new_mode == OutputMode::Tui || self.current_mode == OutputMode::Tui {
            crossterm::execute!(
                std::io::stdout(),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                crossterm::cursor::MoveTo(0, 0)
            )?;
        }
        
        // Create new renderer
        self.current_renderer = new_mode.create_renderer(&self.options);
        self.current_renderer.init().await?;
        
        // Re-render all buffered data in new mode
        for data in &self.event_buffer {
            match data {
                OutputData::CommandMetadata(metadata) => {
                    self.current_renderer.render_command_metadata(metadata).await?;
                }
                OutputData::StackDefinition(def, show_times) => {
                    self.current_renderer.render_stack_definition(def, *show_times).await?;
                }
                // ... re-render all buffered data
            }
        }
        
        self.current_mode = new_mode;
        
        // Show switch notification
        let switch_msg = StatusUpdate {
            message: format!("Switched to {} mode", new_mode),
            timestamp: Utc::now(),
        };
        self.current_renderer.render_status_update(&switch_msg).await?;
        
        Ok(())
    }
}
```

### Keyboard Controls

When running in a TTY environment, users can switch between output modes on-the-fly:

- **`p`**: Switch to Plain mode
- **`i`**: Switch to Interactive mode  
- **`j`**: Switch to JSON mode
- **`t`**: Switch to TUI mode

```rust
pub struct KeyboardListener {
    // Non-blocking keyboard input detection using crossterm
}

impl KeyboardListener {
    pub fn try_read_key(&mut self) -> Result<Option<KeyEvent>> {
        if crossterm::event::poll(Duration::from_millis(0))? {
            if let Event::Key(key) = crossterm::event::read()? {
                if let KeyCode::Char(c) = key.code {
                    return Ok(Some(KeyEvent::Char(c)));
                }
            }
        }
        Ok(None)
    }
}
```

## Usage in Commands

Commands become simple data collection + rendering orchestration:

```rust
// In create_stack command
pub async fn create_stack_main(args: &Args) -> Result<i32> {
    let mut output = DynamicOutputManager::new(args.output_mode, args.output_options()).await?;
    
    // 1. Command metadata with token information (integrated with existing token management)
    let context = create_context(&args.normalized_aws_opts).await?;
    let metadata = collect_command_metadata(args, &context).await?;
    output.render(OutputData::CommandMetadata(metadata)).await?;
    
    // 2. Stack operation  
    let start_time = get_reliable_start_time().await;
    let stack_id = perform_create_stack(&cfn_client, &stack_args).await?;
    
    // 3. Stack definition (exact from implementation spec collect_stack_definition)
    let stack_def = collect_stack_definition(&cfn_client, &stack_id, true).await?;
    output.render(OutputData::StackDefinition(stack_def, true)).await?;
    
    // 4. Previous events (exact from implementation spec collect_stack_events) 
    let previous_events = collect_stack_events(&cfn_client, &stack_id, 10, "Previous Stack Events (max 10):").await?;
    output.render(OutputData::StackEvents(previous_events)).await?;
    
    // 5. Live events with spinner (exact from implementation spec watch_stack_with_output)
    let watcher = StackEventWatcher::new(&cfn_client, &stack_id, start_time);
    watcher.watch_with_output(&mut output).await?;
    
    // 6. Final summary (exact from implementation spec collect_stack_contents)
    let contents = collect_stack_contents(&cfn_client, &stack_id).await?;
    output.render(OutputData::StackContents(contents)).await?;
    
    // 7. Command result (exact from implementation spec)
    let success = is_expected_final_status(&contents.current_status.status);
    let result = CommandResult { 
        success, 
        elapsed_seconds: (Utc::now() - start_time).num_seconds() as u64, 
        message: None 
    };
    output.render(OutputData::CommandResult(result)).await?;
    
    Ok(if result.success { 0 } else { 1 })
}
```

## CLI Integration

```rust
#[derive(Debug, Args)]
pub struct GlobalOpts {
    #[arg(long, value_enum, global = true)]
    #[arg(default_value_t = OutputMode::default_for_environment())]
    #[arg(help = "Output mode for console display")]
    pub output: OutputMode,
    
    #[arg(long, value_enum, global = true, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
    
    #[arg(long, value_enum, global = true, default_value_t = Theme::Auto)]
    pub theme: Theme,
    
    // Existing fields...
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputMode {
    /// Non-interactive text for CI/logs (no spinners)
    Plain,
    /// Interactive text with spinners and colors (exact iidy-js match)
    Interactive,
    /// Machine-readable JSON Lines format
    Json,
    /// Terminal User Interface
    Tui,
}

impl OutputMode {
    pub fn create_renderer(&self, opts: &OutputOptions) -> Box<dyn OutputRenderer> {
        match self {
            OutputMode::Plain => Box::new(PlainTextRenderer::new(opts)),
            OutputMode::Interactive => Box::new(InteractiveRenderer::new(opts)),
            OutputMode::Json => Box::new(JsonRenderer::new()),
            OutputMode::Tui => Box::new(TuiRenderer::new(opts)),
        }
    }
    
    pub fn default_for_environment() -> Self {
        use std::io::IsTerminal;
        
        if std::io::stdout().is_terminal() {
            OutputMode::Interactive
        } else {
            OutputMode::Plain
        }
    }
}
```

## Implementation Strategy

### Phase 1: Core Infrastructure
1. Define data structures from data-driven spec
2. Implement `OutputRenderer` trait
3. Create `DynamicOutputManager` with buffering
4. Add CLI argument for `--output`

### Phase 2: Interactive Renderer (Priority)
1. Implement `InteractiveRenderer` with **exact iidy-js formatting**
2. Use all algorithms from complete implementation spec
3. Match pixel-perfect output including colors, spacing, timing
4. Test against iidy-js output samples

### Phase 3: Plain Text Renderer
1. Implement `PlainTextRenderer` (strip colors/spinners)
2. Maintain same content structure
3. Test in CI environments

### Phase 4: Dynamic Switching
1. Implement keyboard listener with crossterm
2. Add event buffer and mode switching logic
3. Test seamless transitions between modes

### Phase 5: JSON & TUI Renderers
1. Implement `JsonRenderer` with structured output
2. Implement `TuiRenderer` with ratatui
3. Document JSON schema for consumers

## Dependencies

```toml
[dependencies]
# Core output functionality
owo-colors = "4.0"
anstyle = "1.0"
async-trait = "0.1"
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }

# Terminal/input handling
crossterm = "0.27"
terminal_size = "0.3"

# Optional TUI support
ratatui = { version = "0.26", optional = true }

# Tokio integration
tokio = { version = "1", features = ["time", "sync"] }

[features]
default = []
tui = ["ratatui"]
```

## Benefits

1. **Exact Compatibility**: Interactive mode matches iidy-js pixel-perfect
2. **Data Separation**: Clean separation of data collection from presentation
3. **Flexibility**: Multiple output modes for different use cases
4. **Dynamic Switching**: Real-time mode changes with full history replay
5. **Testability**: Mock data structures for comprehensive testing
6. **Maintainability**: Changes to data collection don't affect rendering
7. **Extensibility**: Easy to add new output modes without touching core logic

This architecture ensures we can deliver a pixel-perfect iidy-js experience in Interactive mode while supporting modern use cases like JSON output and dynamic mode switching.

## Integration with Token Management and Testing

This output system integrates seamlessly with the existing **token management system** (from `2025-06-05-token-management-design.md`):

- **Token Display**: Command metadata includes primary and derived tokens with source indication
- **Deterministic Testing**: Fixture-based testing with fixed tokens ensures reproducible output  
- **Multi-Step Operations**: Token derivation displayed during changeset operations
- **Offline Testing**: Complete test coverage without AWS API dependencies

The data-driven architecture enables comprehensive testing across all output modes while maintaining exact iidy-js compatibility and providing the visibility required for production token management.