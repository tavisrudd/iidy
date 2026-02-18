# Output System Architecture

The output system separates data collection from presentation using a
data-driven architecture. Command handlers emit structured `OutputData`
variants; renderers handle all formatting, colors, spinners, and section
management. This enables three output modes (Interactive, Plain, JSON) from
the same command logic, with runtime mode switching.

For the overall system context, see [architecture.md](architecture.md).

## OutputData enum

Defined in `src/output/data.rs`, `OutputData` is the intermediate
representation between command handlers and renderers. Each variant carries
a payload struct with the data needed for rendering. Key variants:

| Variant | Purpose |
|---------|---------|
| `CommandMetadata` | Operation type, environment, region, credentials |
| `StackDefinition` | Stack name, parameters, tags, template info |
| `NewStackEvents` | Batch of CloudFormation stack events |
| `StackContents` | Resources, outputs, exports of a stack |
| `StackEvents` | Historical events (before current operation) |
| `OperationComplete` | Elapsed time, skip-remaining flag |
| `ConfirmationPrompt` | Message + `oneshot::Sender` for response |
| `Error` | Structured error info for rendering |
| `ChangeSetResult` | Changeset creation result for review |
| `TemplateDiff` | Diff output for template approval |

The full enum has 25 variants covering all CloudFormation operations.

## Renderers

### OutputRenderer trait

Defined in `src/output/renderer.rs`:

```rust
#[async_trait]
pub trait OutputRenderer: Send + Sync {
    async fn init(&mut self) -> Result<()>;
    async fn cleanup(&mut self) -> Result<()>;
    async fn render_output_data(
        &mut self,
        data: OutputData,
        buffer: Option<&VecDeque<OutputData>>,
    ) -> Result<()>;
}
```

### InteractiveRenderer

`src/output/renderers/interactive.rs`. Handles:
- Section headings with consistent formatting
- Spinners with timing updates (via `indicatif`)
- Color theming through `IidyTheme` (`src/output/theme.rs`)
- Out-of-order section buffering
- Live event streaming with spinner coordination
- Confirmation prompts via crossterm

### JsonRenderer

`src/output/renderers/json.rs`. Outputs one JSON object per event in JSONL
format with `type` and `timestamp` fields. Non-interactive: confirmation
prompts are always declined.

### Plain mode

Not a separate renderer -- it is InteractiveRenderer configured with:
- `color_choice: ColorChoice::Never`
- `enable_spinners: false`
- `enable_ansi_features: false`

Note: `show_timestamps` is true for both Interactive and Plain modes.

## Section management

Each CloudFormation operation has a predefined list of expected sections.
The interactive renderer assigns these in `start_operation()`:

```rust
match operation {
    CfnOperation::CreateStack => vec![
        "command_metadata", "stack_definition",
        "live_stack_events", "stack_contents"
    ],
    CfnOperation::DeleteStack => vec![
        "command_metadata", "stack_definition",
        "stack_events", "stack_contents",
        "confirmation", "live_stack_events"
    ],
    // ...
}
```

Sections are rendered in order. When data arrives out of order (because
parallel API calls complete in unpredictable order), the renderer buffers
it in `pending_sections` and advances through ready sections:

```rust
async fn advance_through_ready_sections(&mut self) -> Result<()> {
    while self.next_section_index < self.expected_sections.len() {
        let key = self.expected_sections[self.next_section_index];
        if let Some(data) = self.pending_sections.remove(key) {
            self.render_section(data);
            self.next_section_index += 1;
        } else {
            break; // Wait for this section's data
        }
    }
}
```

## Spinner lifecycle

Spinners are managed entirely by the renderer. Command handlers never
start, stop, or reference spinners.

1. **Start**: When `start_next_section()` is called, a spinner is created
   with "Loading {section}..." text if `enable_spinners` is true.
2. **Timing updates**: Spinners show elapsed time during long operations.
3. **Clear on data**: When section data arrives, the spinner is cleared
   before rendering.
4. **Clear on complete**: `OperationComplete` clears any active spinner.
5. **Clear on skip**: When `skip_remaining_sections` is true, remaining
   spinners are cleaned up without rendering their sections.

## Mode switching

`DynamicOutputManager` in `src/output/manager.rs` enables runtime mode
switching between Interactive, Plain, and JSON.

It maintains a `VecDeque<OutputData>` buffer (max 1000 events). When the
user triggers a mode switch (via keyboard listener), the manager:

1. Cleans up the current renderer
2. Creates a new renderer for the target mode
3. Replays the entire buffer through the new renderer
4. Continues routing new events to the new renderer

This allows switching from Interactive to JSON mid-operation without losing
any output.

## Live events streaming

`live_stack_events` is a special streaming section. Unlike other sections
that receive a single `OutputData`, live events arrive incrementally as
CloudFormation processes resources.

The renderer handles this by:
1. Advancing to the live events section when ready
2. For each `NewStackEvents` batch: clearing the spinner, rendering events,
   restarting the spinner
3. Buffering events that arrive before the section is reached
4. Stopping the timing task on `LiveStackEventsComplete` or
   `OperationComplete`

## Confirmation handling

Confirmations flow through the output pipeline like any other data:

**Command handler perspective** -- a simple async call:
```rust
let confirmed = output_manager.request_confirmation(
    "Delete this stack?".to_string()
).await?;
```

**Output manager** -- creates a `oneshot::channel` internally, wraps it in
a `ConfirmationPrompt` variant, sends it through the renderer, and awaits
the response.

**Interactive renderer** -- displays the prompt, reads keyboard input,
sends the response through the channel.

**JSON renderer** -- always returns false (non-interactive).

## Command handler rules

Command handlers must:
- Push `OutputData` variants to `DynamicOutputManager`
- Handle AWS API orchestration and error conversion
- Send `OperationComplete` when done, with `skip_remaining_sections` if
  appropriate (e.g., stack was deleted, so skip `stack_contents`)

Command handlers must NOT:
- Format text, choose colors, or produce ANSI output
- Start, stop, or reference spinners
- Know about section titles or ordering
- Decide display layout or timing
- Interact directly with renderers (only through `DynamicOutputManager`)
