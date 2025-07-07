# ADR: Output Sequencing, Formatting, and Theming Architecture

**Date:** 2025-07-06  
**Updated:** 2025-07-07  
**Status:** Implemented  
**Supersedes:** `notes/2025-06-17-data-driven-output-architecture.md`  

## Context and Problem Statement

The iidy Rust rewrite provides three distinct output modes:

1. **Interactive Mode**: Rich colored output with spinners, proper section ordering, and real-time feedback
2. **Plain Mode**: CI-friendly output with timestamps, no colors, no spinners, but same information structure
3. **JSON Mode**: Machine-readable JSONL format for automation and integration

The original architecture document became outdated as implementation evolved. We need to document the actual patterns we've adopted for output sequencing, theming, and mode management.

## Key Architectural Motivations

### 1. **Separation of Concerns**
Command handlers should focus purely on AWS API orchestration and data collection. All presentation logic (formatting, colors, spinners, section ordering) belongs in renderers. This separation enables:
- Multiple output modes from the same command logic
- Consistent AWS API patterns across commands
- Testable command handlers using fixture data instead of real AWS calls

### 2. **Consistent Output Formatting**
The interactive renderer provides well-structured output with:
- Coherent color schemes and theming
- Proper column alignment and spacing
- Logical section headings and ordering
- Appropriate spinner behavior and timing messages
- Clear error formatting and structured output

### 3. **Robust Section Sequencing**
CloudFormation operations involve multiple async data sources (stack metadata, events, resources) that may complete out of order. The renderer must handle this complexity while presenting data in the correct logical sequence.

## Decision: Multi-Layer Architecture

This architecture uses a clean sequential await pattern for handling multiple concurrent AWS API calls.

### Layer 1: Command Handlers (`src/cfn/`)

**Responsibilities:**
- AWS API orchestration and error handling
- Data collection from multiple concurrent sources
- Converting AWS SDK types to our `OutputData` enum
- Sending data to `DynamicOutputManager` as it becomes available
- **NO formatting, colors, section titles, or spinner management**

**Current Implementation: Macro-Based Error Handling**

All command handlers currently use the `await_and_render!` macro for consistent error handling and exit code management:

**Parallel Operations Pattern (Spawn + Progressive Rendering):**
```rust
// For operations with multiple independent API calls that benefit from parallelism
let stack_task = tokio::spawn(async move { /* fetch stack data */ });
let events_task = tokio::spawn(async move { /* fetch events data */ });
let contents_task = tokio::spawn(async move { /* fetch contents data */ });

// Use await_and_render! for consistent error handling and exit codes
await_and_render!(stack_task, output_manager);   // Renders as soon as ready
await_and_render!(events_task, output_manager);  // May already be complete
await_and_render!(contents_task, output_manager); // May already be complete

// The await_and_render! macro handles the double-Result pattern from tokio::spawn:
macro_rules! await_and_render {
    ($task:expr, $output_manager:expr) => {
        match $task.await {
            Ok(Ok(data)) => $output_manager.render(data).await?,
            Ok(Err(error)) => {
                let error_info = convert_aws_error_to_error_info(&error);
                $output_manager.render(OutputData::Error(error_info)).await?;
                return Ok(1);
            }
            Err(join_error) => {
                let error_info = convert_aws_error_to_error_info(&join_error.into());
                $output_manager.render(OutputData::Error(error_info)).await?;
                return Ok(1);
            }
        }
    };
}
```

**Single Operation Pattern (Manual Error Handling):**
```rust
// For single operations, currently using manual error handling patterns
match StackInfoService::get_stack(&context.client, &stack_id).await {
    Ok(stack) => {
        let output_data = convert_stack_to_definition(&stack, true);
        output_manager.render(output_data).await?;
    }
    Err(error) => {
        let error_info = convert_aws_error_to_error_info(&error);
        output_manager.render(OutputData::Error(error_info)).await?;
        return Ok(1);
    }
}
```

**Future Direction: Handler Wrapper Function**

We plan to introduce a cleaner wrapper-based approach that eliminates repetitive error handling code while supporting explicit exit codes for cases like user cancellation:

```rust
pub async fn execute_with_error_handling<F, Fut>(
    output_manager: &mut DynamicOutputManager,
    handler: F,
) -> Result<i32> 
where
    F: FnOnce(&mut DynamicOutputManager) -> Fut,
    Fut: std::future::Future<Output = Result<i32>>,
{
    match handler(output_manager).await {
        Ok(exit_code) => Ok(exit_code), // Handler returned explicit exit code
        Err(error) => {
            // Convert error and render through output system
            let error_info = convert_aws_error_to_error_info(&error);
            output_manager.render(OutputData::Error(error_info)).await?;
            Ok(1) // Error exit code
        }
    }
}
```

**Future Handler Pattern:**
```rust
pub async fn my_command(cli: &Cli, args: &MyArgs) -> Result<i32> {
    let mut output_manager = DynamicOutputManager::new(...).await?;
    
    execute_with_error_handling(&mut output_manager, |output_manager| async move {
        // Command logic with standard Rust error handling
        let confirmed = output_manager.request_confirmation("Delete?").await?;
        if !confirmed {
            return Ok(130); // User cancellation - explicit exit code
        }
        
        // For parallel operations, use helper to unwrap task results
        let stack_data = unwrap_task(stack_task).await?;
        output_manager.render(stack_data).await?;
        
        Ok(0) // Success
    }).await
}

// Helper to unwrap tokio::spawn double-Result pattern
async fn unwrap_task<T>(task: tokio::task::JoinHandle<Result<T, anyhow::Error>>) -> Result<T> {
    match task.await {
        Ok(Ok(data)) => Ok(data),
        Ok(Err(error)) => Err(error),
        Err(join_error) => Err(join_error.into()),
    }
}
```

**Migration Strategy:**
- **Phase 1**: Continue using `await_and_render!` macro for all current handlers
- **Phase 2**: Introduce `execute_with_error_handling` wrapper for new commands
- **Phase 3**: Gradually migrate existing handlers when touching them for other reasons
- **No mass migration**: Existing working code remains unchanged unless there's a specific need

### Layer 2: Output Manager (`src/output/manager.rs`)

**Responsibilities:**
- Mode switching between Interactive/Plain/JSON
- Event buffering for mode transitions  
- Routing data to appropriate renderer
- **Interface abstraction**: Hides rendering complexity from command handlers

**Key Features:**
- **Buffer Management**: Keeps last 1000 events for seamless mode switching
- **Clean Confirmation API**: Command handlers call simple `request_confirmation(message)`, manager handles `oneshot::channel` internally
- **Single Interface**: Command handlers only interact with `DynamicOutputManager`, never directly with renderers
- **Direct Rendering**: Simple `render()` method for all output data

**Command Handler Interface:**
```rust
// Direct rendering
output_manager.render(OutputData::StackDefinition(stack_def)).await?;
let confirmed = output_manager.request_confirmation("Delete stack?".to_string()).await?;
```

**Confirmation Implementation:**
```rust
// Manager hides oneshot::channel complexity from command handlers
async fn request_confirmation_impl(&mut self, message: String, key: Option<String>) -> Result<bool> {
    let (response_tx, response_rx) = oneshot::channel(); // Hidden from command handlers
    let confirmation = OutputData::ConfirmationPrompt(ConfirmationRequest {
        message,
        response_tx: Some(response_tx), // Manager handles channel creation
        key,
    });
    self.render(confirmation).await?;
    response_rx.await.map_err(|_| anyhow::anyhow!("Confirmation response channel closed"))
}
```

### Layer 3: Renderers (`src/output/renderers/`)

#### Interactive Renderer (`interactive.rs`)

**Responsibilities:**
- Section sequencing with expected section arrays per operation
- Spinner lifecycle management (start, timing updates, cleanup)
- Consistent formatting and colors via `IidyTheme`
- Asynchronous ordering handling (buffering out-of-order sections)

**Section Sequencing Pattern:**
```rust
// Pre-defined sections per operation
match operation {
    CfnOperation::CreateStack => vec!["command_metadata", "stack_definition", "live_stack_events", "stack_contents"],
    CfnOperation::DeleteStack => vec!["command_metadata", "stack_definition", "stack_events", "stack_contents", "confirmation", "live_stack_events"],
    // ...
}

// Sections shown immediately with spinners, data rendered when available
fn start_next_section(&mut self) {
    let section_key = self.expected_sections[self.next_section_index];
    let title = self.get_section_title(section_key);
    self.print_section_heading(&title);
    if self.options.enable_spinners {
        self.current_spinner = self.create_api_spinner(&format!("Loading {}...", title.to_lowercase()));
    }
}
```

**Theme Integration:**
- Uses `IidyTheme` in `src/output/theme.rs` for consistent color schemes
- **NOT** the unused `ColorContext` in `src/output/color.rs`
- Direct color application: `text.color(self.theme.primary).to_string()`

#### JSON Renderer (`json.rs`)

**Responsibilities:**
- JSONL format output with type and timestamp metadata
- Raw JSON mode for certain operations (like `list-stacks --query`)
- Non-interactive confirmation handling (always decline)

#### Plain Mode
Implemented as **Interactive Renderer with plain configuration**:
```rust
OutputMode::Plain => {
    let interactive_options = InteractiveOptions {
        color_choice: ColorChoice::Never, // Force no colors
        enable_spinners: false,           // No spinners 
        enable_ansi_features: false,      // No ANSI features
        show_timestamps: true,            // Keep timestamps for CI
        // ...
    };
}
```

### Layer 4: Theme System (`src/output/theme.rs`)

**Responsibilities:**
- Consistent color mappings for professional terminal output
- Terminal capability detection and color choice handling
- Semantic color naming (success, error, warning, etc.)

**Implementation:**
```rust
pub struct IidyTheme {
    pub colors_enabled: bool,
    pub primary: owo_colors::Color,        // Blue for key information
    pub success: owo_colors::Color,        // Green for success states
    pub error: owo_colors::Color,          // Red for errors/failures
    pub warning: owo_colors::Color,        // Yellow for warnings/progress
    // ... comprehensive color scheme
}
```

## Implementation Patterns

### 1. Asynchronous Section Ordering

**Problem**: AWS API calls complete out of order, but sections must display in logical sequence.

**Solution**: Buffer pending sections and advance when ready:
```rust
async fn render_with_ordering(&mut self, data: OutputData) -> Result<()> {
    let section_key = self.get_section_key(&data);
    if let Some(key) = section_key {
        self.pending_sections.insert(key, data);
        self.advance_through_ready_sections().await?;
    }
}

async fn advance_through_ready_sections(&mut self) -> Result<()> {
    while self.next_section_index < self.expected_sections.len() {
        let section_key = self.expected_sections[self.next_section_index];
        if let Some(data) = self.pending_sections.remove(&section_key.to_string()) {
            self.render_section(data).await?;
            self.next_section_index += 1;
        } else {
            break; // Wait for this section's data
        }
    }
}
```

### 2. Live Events Streaming

**Special Case**: `live_stack_events` is a streaming section that displays events as they arrive:
```rust
async fn handle_live_events_data(&mut self, data: OutputData) -> Result<()> {
    // Advance to live_stack_events section if ready
    self.advance_through_ready_sections().await?;
    
    if self.current_section_is_live_events() {
        // Clear spinner, render event, restart spinner
        self.stop_live_events_timing();
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }
        self.render_data_immediately(data).await?;
        // Restart spinner for continued polling
        self.current_spinner = self.create_api_spinner("Loading live events...");
        self.start_live_events_timing(start_time);
    } else {
        // Buffer for later
        self.buffered_live_events.push(data);
    }
}
```

### 3. Confirmation Handling

**Command Handler Perspective** - Simple synchronous-looking call:
```rust
// Command handlers get a clean, simple API
let confirmed = output_manager.request_confirmation("Delete this stack?".to_string()).await?;
if !confirmed {
    return Ok(1); // Handle user cancellation
}
```

**Output Manager Implementation** - Handles all async channel complexity internally:
```rust
// DynamicOutputManager hides oneshot::channel from command handlers
async fn request_confirmation_impl(&mut self, message: String, key: Option<String>) -> Result<bool> {
    let (response_tx, response_rx) = oneshot::channel(); // Created internally
    let confirmation = OutputData::ConfirmationPrompt(ConfirmationRequest {
        message,
        response_tx: Some(response_tx), // Passed to renderer
        key,
    });
    
    self.render(confirmation).await?; // Send through normal rendering system
    response_rx.await.map_err(|_| anyhow::anyhow!("Confirmation response channel closed"))
}
```

**Benefits of this abstraction:**
- Command handlers don't need to understand `oneshot::channel` or async coordination
- Confirmation integrates seamlessly with section ordering and spinners
- Different renderers can handle confirmation differently (interactive vs JSON mode)

### 4. Operation Completion and Cleanup

**Pattern**: Commands signal completion, renderers handle cleanup:
```rust
// Command handler signals completion
let completion = OutputData::OperationComplete(OperationCompleteInfo {
    elapsed_seconds: total_time,
    skip_remaining_sections: stack_was_deleted, // Skip stack_contents if deleted
});
output_manager.render(completion).await?;

// Renderer handles cleanup
async fn handle_operation_complete(&mut self, info: &OperationCompleteInfo) -> Result<()> {
    self.stop_live_events_timing();
    if let Some(spinner) = self.current_spinner.take() {
        spinner.clear();
    }
    
    if info.skip_remaining_sections {
        self.cleanup_operation();
    } else {
        self.advance_through_ready_sections().await?;
    }
}
```

## Why Not Use ColorContext?

The `ColorContext` in `src/output/color.rs` was an early design that became unused. The current architecture embeds color management directly in the `InteractiveRenderer` via `IidyTheme` because:

1. **Renderer-Specific**: Each renderer has different color needs (Interactive uses colors, JSON ignores them, Plain disables them)
2. **Focused Design**: `IidyTheme` provides the specific color mappings needed for our output formatting
3. **Simpler Integration**: Direct theme usage in renderers avoids global state and matches the data-driven pattern

## Benefits of This Architecture

### 1. **Multiple Output Modes**
Same command logic produces Interactive, Plain, and JSON output with mode switching support.

### 2. **Right-Sized Async Patterns**
Uses `spawn` for true parallelism with progressive rendering (like `describe_stack`), direct await for simple sequential operations. No unnecessary task overhead.

### 3. **Professional Output Quality**
Well-structured, consistent output via dedicated theme system and precise formatting logic.

### 4. **Testable Components**
Command handlers can be tested with fixture data. Renderers can be tested with mock data structures.

### 5. **Clean Interface Boundaries**
- Command handlers only interact with `DynamicOutputManager` - never directly with renderers
- Manager abstracts away rendering complexity (confirmation channels, mode switching)
- Clear separation between AWS API logic and presentation logic
- Changes to output formatting don't affect command handlers

### 6. **Optimal Async Patterns**
- **Spawn for parallelism**: Multiple independent API calls with progressive rendering
- **Direct await for simplicity**: Single operations without unnecessary task overhead
- **Progressive rendering**: Users see data as soon as each API call completes
- **True background execution**: Operations start immediately and run concurrently

## Trade-offs

### Right-Sized Complexity
Uses the simplest pattern for each use case: direct await for single operations, spawn for true parallelism with progressive rendering.

### Memory Usage
Event buffering for mode switching uses memory, but limited to 1000 events and only for operations that support mode switching.

### Learning Curve
Uses standard Rust async patterns that are familiar to developers: direct await and spawn when actually needed.

## Conclusion

This architecture successfully provides professional, consistent output while supporting multiple output modes and robust async operations. The separation of concerns enables independent testing and maintenance of AWS API logic versus presentation logic.

**Current Implementation**: All CloudFormation command handlers use the `await_and_render!` macro for consistent error handling:

- **Multi-operation commands** (`describe_stack`, `delete_stack`): Use `spawn` for true parallelism with progressive rendering
- **Operation with monitoring** (`create_stack`, `update_stack`, `create_or_update`, `exec_changeset`, `watch_stack`): Use `spawn` for stack definition + sequential await for live monitoring

**Evolution Path**: We're transitioning toward wrapper-based error handling:

- **Current**: `await_and_render!` macro handles tokio::spawn double-Result pattern consistently
- **Future**: `execute_with_error_handling` wrapper function for cleaner code with explicit exit code support
- **Migration**: Gradual adoption without mass changes to working code

**Key Insights:**

1. **`spawn` is valuable for parallel + progressive rendering**: When you have multiple independent API calls and want to render results as soon as each completes
2. **Wrapper functions reduce boilerplate**: Eliminate repetitive error handling while supporting explicit exit codes (130 for cancellation)
3. **Rust futures are lazy**: Unlike JavaScript promises, they don't start until polled, making direct await truly sequential
4. **Progressive rendering requires spawn**: For the "render as ready" UX pattern, background task execution is necessary
5. **Error handling consistency**: All errors go through the same conversion and rendering pipeline for professional output

This provides the optimal balance of performance, simplicity, and user experience across all CloudFormation operations, with a clear evolution path toward even cleaner error handling patterns.