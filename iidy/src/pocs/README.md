# iidy Proof of Concepts (POCs)

This directory contains proof-of-concept demonstrations and experimental features for iidy.

## Running POCs

Use the dedicated `iidy-pocs` binary to run demonstrations:

```bash
# Build both binaries
cargo build

# Run a specific demo
cargo run --bin iidy-pocs detect-background
cargo run --bin iidy-pocs theme-demo
cargo run --bin iidy-pocs spinner-demo
cargo run --bin iidy-pocs ratatui-demo

# See all available demos
cargo run --bin iidy-pocs --help
```

## Available Demonstrations

### `detect-background`
Terminal background detection and theme validation featuring:
- Multiple detection strategies (COLORFGBG, TERM_PROGRAM, theme variables)
- Comprehensive environment variable analysis
- Detection strategy voting and priority system
- Theme color comparison across Dark, Light, and High Contrast themes
- Testing recommendations for different terminal configurations
- Validates the auto-detection logic used by `--theme=auto`

### `theme-demo`
Demonstrates the color theming system including:
- Terminal capability detection (TTY, 24-bit color, width)
- All available color themes (Auto, Light, Dark, High Contrast)
- Semantic color markup (success, error, warning, info, etc.)
- CloudFormation status colorization
- Progress indicators and spinners

### `spinner-demo`
Showcases the ora-like progress indicator API:
- Different spinner styles (Dots, Dots12, Line, Arrow, Pulse)
- Spinner lifecycle methods (succeed, fail, warn, info)
- Dynamic text updates during operations
- CloudFormation operation simulation

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
- `detect_background.rs` - Terminal background detection and theme validation
- `theme_demo.rs` - Color theming demonstrations
- `spinner_demo.rs` - Progress indicator demonstrations  
- `ratatui_demo.rs` - Terminal UI demonstrations

## Development

The POCs use the main iidy library and serve as both demonstrations and integration tests for new features before they're incorporated into the main CLI.