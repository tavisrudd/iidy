# ADR-002: Data-Driven Output Architecture

Status: Accepted
Date: 2025-06-17

## Context

The original iidy (JavaScript) coupled output formatting directly to the logic that made AWS
API calls. Each operation called display functions inline, making it impossible to produce
alternative output formats (JSON, plain text) without duplicating the entire control flow.
Unit testing required either live AWS calls or complex mocking of display side effects.

For the Rust rewrite, we needed command handlers to be testable with offline fixture data,
and we needed a clean path to support interactive, plain, and JSON output modes from the
same handler implementation. The key constraint was that the set of displayable data types
is known at compile time -- we are representing a fixed domain (CloudFormation operations)
rather than a generic plugin system.

## Decision

All output is expressed as values of the `OutputData` enum, defined in `src/output/data.rs`.
Each variant carries a typed struct capturing exactly what must be displayed for one logical
unit of output (e.g., `OutputData::StackDefinition(StackDefinition, bool)`,
`OutputData::StackEvents(StackEventsDisplay)`, `OutputData::Error(ErrorInfo)`). As of the
current implementation, the enum has 25 variants covering the full set of CloudFormation
operation outputs.

Command handlers produce `OutputData` values and pass them to `DynamicOutputManager::render()`.
The manager dispatches to the active renderer. Renderers -- `InteractiveRenderer`, `JsonRenderer`,
and plain (which is `InteractiveRenderer` with `ColorChoice::Never` and spinners disabled) --
each implement the full set of variant handlers independently. This means rendering logic is
fully decoupled from data collection logic.

Structs in `data.rs` derive `Clone`, `Debug`, `Serialize`, and `Deserialize`. Serialization
allows the JSON renderer to emit any variant as JSONL without renderer-specific marshalling
code. The `ConfirmationRequest` struct is a deliberate exception: it carries a
`oneshot::Sender<bool>` which cannot be serialized, and its `Clone` implementation sets the
sender to `None` to satisfy the derive requirement without making the channel usable after
cloning.

## Consequences

Command handlers are testable in isolation using fixture data: construct an `OutputData`
value, assert on its fields, or feed it through a renderer with captured output. No AWS
credentials or network access required for output formatting tests.

Adding a new output type requires: defining a struct in `data.rs`, adding a variant to
`OutputData`, adding a `data_type()` arm, and implementing the variant in each renderer.
This is intentionally mechanical -- the compiler enforces completeness via exhaustive match.

The intermediate representation does add one allocation per rendered item, which is
acceptable given that rendering is dominated by I/O cost. The `OutputData` enum is not
extensible by external crates, which is correct: iidy's output types are internal to the
tool and not part of a public API.

The `ColorContext` type in `src/output/color.rs` provides global color configuration,
initialized via `ColorContext::init_global()` in `main.rs`. It manages color enablement,
theme selection, and terminal capabilities. `IidyTheme` (`src/output/theme.rs`) is
embedded in `InteractiveRenderer` for renderer-specific styling.
