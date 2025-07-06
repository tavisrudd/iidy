# ADR: Output Sequencing, Formatting, and Theming Architecture

**Date:** 2025-07-06  
**Status:** Adopted  
**Supersedes:** `notes/2025-06-17-data-driven-output-architecture.md`  

## Context and Problem Statement

The iidy Rust rewrite provides three distinct output modes. Initially we aimed for pixel-perfect compatibility with the original iidy-js implementation, which we achieved and have now moved beyond:

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

**UPDATE 2025-07-06**: We are migrating away from the parallel channel pattern towards a simpler sequential await approach that maintains parallel execution benefits while eliminating coordination complexity.

### Layer 1: Command Handlers (`src/cfn/`)

**Responsibilities:**
- AWS API orchestration and error handling
- Data collection from multiple concurrent sources
- Converting AWS SDK types to our `OutputData` enum
- Sending data to `DynamicOutputManager` as it becomes available
- **NO formatting, colors, section titles, or spinner management**

**Current Pattern (being phased out):**
```rust
// OLD: Channel-based parallel coordination
let sender = output_manager.start();
let stack_task = spawn_stack_collection_task(&sender, &client, &stack_id);
let events_task = spawn_events_watching_task(&sender, &client, &stack_id);
output_manager.stop().await?; // Processes all parallel data in arrival order
```

**New Preferred Pattern:**
```rust
// NEW: Sequential await with parallel execution
let stack_task = tokio::spawn(async move { /* fetch stack data */ });
let events_task = tokio::spawn(async move { /* fetch events data */ });
let contents_task = tokio::spawn(async move { /* fetch contents data */ });

// Await and render in correct section order (tasks already running in parallel)
await_and_render!(stack_task, output_manager);
await_and_render!(events_task, output_manager);
await_and_render!(contents_task, output_manager);

// Macro handles error rendering consistently:
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
- **Simplified Coordination**: No longer requires parallel channel management - command handlers use direct `render()` calls

**Simplified Command Handler Interface:**
```rust
// Direct rendering - no channel coordination needed
output_manager.render(OutputData::StackDefinition(stack_def)).await?;
let confirmed = output_manager.request_confirmation("Delete stack?".to_string()).await?;
```

**Confirmation Implementation (still uses channels internally):**
```rust
// Manager still hides oneshot::channel complexity from command handlers
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

### 2. **Simplified Async Handling**
Parallel AWS API calls with direct sequential rendering eliminates channel coordination complexity while maintaining performance benefits.

### 3. **Professional Output Quality**
Well-structured, consistent output via dedicated theme system and precise formatting logic.

### 4. **Testable Components**
Command handlers can be tested with fixture data. Renderers can be tested with mock data structures.

### 5. **Clean Interface Boundaries**
- Command handlers only interact with `DynamicOutputManager` - never directly with renderers
- Manager abstracts away rendering complexity (confirmation channels, mode switching)
- Clear separation between AWS API logic and presentation logic
- Changes to output formatting don't affect command handlers

### 6. **Simplified Coordination**
The new pattern eliminates the need for:
- Parallel channel infrastructure (`start()/stop()` methods)
- Complex async ordering logic in renderers
- Channel-based error handling coordination

While maintaining:
- Parallel AWS API execution for performance
- Immediate rendering as data becomes available  
- Proper error handling via the output system
- Consistent exit code tracking

## Trade-offs

### Complexity (Reduced)
The new sequential await pattern significantly reduces complexity compared to the channel-based approach, while maintaining all functional benefits.

### Memory Usage
Event buffering for mode switching uses memory, but limited to 1000 events and only for operations that support mode switching.

### Learning Curve (Improved)
The new pattern is more intuitive for developers familiar with async/await patterns, reducing the learning curve compared to channel coordination.

## Conclusion

This architecture successfully provides professional, consistent output while supporting multiple output modes and robust async operations. The separation of concerns enables independent testing and maintenance of AWS API logic versus presentation logic.

**Migration Strategy**: We are migrating from the channel-based parallel coordination pattern to the simpler sequential await pattern. This transition:
- Reduces implementation complexity by ~50 lines of coordination code per command
- Eliminates the need for `start()/stop()` methods in `DynamicOutputManager`
- Maintains all performance and user experience benefits
- Provides cleaner error handling via standardized macros
- Makes the codebase more approachable for new developers

The key insight is that while CloudFormation operations have inherent sequencing requirements, these can be handled more simply through direct sequential awaiting rather than complex channel coordination, while still maintaining parallel AWS API execution for optimal performance.