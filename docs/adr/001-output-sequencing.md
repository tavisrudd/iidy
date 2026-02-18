# ADR-001: Output Sequencing Architecture

Status: Accepted
Date: 2025-07-06

## Context

CloudFormation operations require data from multiple independent AWS API calls: stack metadata,
event history, resource lists, changeset details. These calls are issued concurrently for
latency, but output must appear in a fixed logical order regardless of which call completes
first. Additionally, iidy must support three output modes -- interactive (colored, spinners),
plain (CI-safe, timestamps), and JSON (JSONL, machine-readable) -- from the same command
handler code, with seamless switching between modes at runtime.

The naive approach -- formatting output directly inside command handlers -- creates tight
coupling between AWS API logic and presentation logic, makes unit testing impossible without
live AWS credentials, and forces a single output mode per handler.

## Decision

A three-layer architecture separates concerns cleanly:

**Layer 1 -- Command handlers (`src/cfn/`)**: Responsible only for AWS API orchestration.
Handlers issue concurrent API calls via `tokio::spawn`, convert SDK types to `OutputData`
enum variants, and call `output_manager.render()` as each call completes. Handlers never
reference colors, spinners, section titles, or display order.

**Layer 2 -- Output manager (`src/output/manager.rs`)**: A `DynamicOutputManager` sits
between handlers and renderers. It routes each `OutputData` value to the active renderer,
buffers up to 1000 events for mode-switching transitions, and hides async coordination
primitives (e.g., `oneshot::channel` for confirmation prompts) from handlers. Handlers call
`output_manager.request_confirmation(message)` and receive a `bool`; the channel mechanics
are internal to the manager.

**Layer 3 -- Renderers (`src/output/renderers/`)**: Renderers own all presentation logic.
The interactive renderer maintains an ordered list of expected sections per operation (e.g.,
`["command_metadata", "stack_definition", "live_stack_events", "stack_contents"]`). When an
`OutputData` value arrives for a section that is not yet next in sequence, the renderer
buffers it in a `pending_sections` map and waits. As each expected section's data arrives,
the renderer advances in order: printing the section heading, clearing the previous spinner,
and rendering the data. Plain mode reuses the interactive renderer with colors and spinners
disabled. JSON mode emits JSONL with type and timestamp metadata.

New command handlers use the `run_command_handler!` macro, which centralizes AWS setup,
output manager creation, and error conversion. Existing handlers use the `await_and_render!`
macro and are migrated opportunistically.

## Consequences

This architecture enables independent testing of command handlers using fixture data without
AWS credentials. The same handler code produces interactive, plain, and JSON output without
modification. Section ordering is guaranteed regardless of which API call completes first.

The trade-offs are real. The indirection through `OutputData` adds a conversion step between
SDK types and display. Section ordering in the renderer must be kept in sync with the
sections a handler actually emits -- a mismatch causes silent buffering. The 1000-event
buffer for mode switching has a memory cost proportional to operation duration.

The `run_command_handler!` migration is incremental: handlers that have not been touched
since the macro was introduced still use `await_and_render!`, so two patterns coexist during
the transition.
