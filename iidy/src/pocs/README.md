# iidy Proof of Concepts (POCs)

This directory contains proof-of-concept demonstrations and
experimental features for iidy. We remove them after the experiment or
feature they are related to has been completed.

## Running POCs

Use the dedicated `iidy-pocs` binary to run demonstrations:

```bash
# Build both binaries
cargo build

# Run a specific demo
cargo run --bin iidy-pocs foobar-demo

# See all available demos
cargo run --bin iidy-pocs --help
```

## Available Demonstrations

### `ratatui-demo`
Interactive terminal UI demonstration featuring:
- Real-time CloudFormation stack monitoring simulation
- Tabbed interface (Stack Info, Events, Resources)
- Sortable tables with mouse and keyboard support
- Row selection and highlighting
- Live event streaming every 3 seconds
- Mouse-clickable tabs and columns
- Keyboard shortcuts for all operations

## Module Structure

- `main.rs` - POCs binary entry point
- `mod.rs` - Module declarations
- `ratatui_demo.rs` - Terminal UI demonstrations

## Development

The POCs use the main iidy library and serve as both demonstrations and integration tests for new features before they're incorporated into the main CLI.
