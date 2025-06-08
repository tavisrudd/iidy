# Color Theming and Terminal Output Design

**Date:** 2025-06-07  
**Status:** Design Spike  
**Priority:** Medium

## Overview

This document outlines the design for implementing configurable color schemes and enhanced terminal output for the iidy Rust CLI tool. The system should respect existing ColorChoice options, add configurable themes, support 24-bit color when available, and include progress indicators for CloudFormation operations.

## Current State Analysis

### Existing Implementation (cli.rs:82-83)
```rust
#[arg(long, value_enum, global = true, default_value_t = ColorChoice::Auto, help = "Whether to color output using ANSI escape codes")]
pub color: ColorChoice,
```

Current ColorChoice enum supports:
- `Auto`: Should detect TTY and check NO_COLOR environment variable
- `Always`: Force color output
- `Never`: Disable color output

### Color Usage Patterns from iidy-js Reference

From analysis of the JavaScript implementation, colors are used for:

1. **Status Indicators**: Resource states (in-progress=yellow, complete=green, failed=red, skipped=blue)
2. **Timestamps**: Muted gray formatting
3. **Section Headers**: Bold white formatting  
4. **Resource Identifiers**: Light gray for logical IDs
5. **Success/Failure Messages**: Green backgrounds, red backgrounds with emoji
6. **Progress Reasons**: Red for failures, muted for normal reasons
7. **Spinner**: TTY-only progress indication during CloudFormation operations

Key patterns identified:
- Heavy use of `cli-color` with xterm 256-color support
- TTY detection for spinner support
- Column alignment with padding calculations
- Text wrapping aware of terminal width
- Status-based color coding (semantic colors, not just aesthetic)

## Requirements

### Functional Requirements

1. **Color Configuration**
   - Extend existing `--color` flag to respect TTY and NO_COLOR
   - Add `--theme` argument for configurable color schemes
   - Default themes: `auto`, `light`, `dark`, `high-contrast`
   - Support for 24-bit true color when terminal supports it

2. **Terminal Detection**
   - TTY detection for `--color=auto`
   - NO_COLOR environment variable support
   - COLORTERM environment variable detection for 24-bit support
   - Terminal width detection for column alignment

3. **Progress Indication**
   - Spinner for TTY mode during CloudFormation operations
   - Fallback to periodic text updates for non-TTY
   - Multi-operation progress tracking

4. **Output Formatting**
   - Semantic color markup throughout codebase
   - Column alignment with auto-width adjustment
   - Text wrapping for terminal width
   - Consistent spacing and padding

### Non-Functional Requirements

1. **Performance**: Zero-allocation color formatting where possible
2. **Compatibility**: Integration with existing clap styling
3. **Maintainability**: Markup-based approach for easy theme changes
4. **Accessibility**: High-contrast theme option

## Technical Design

### Recommended Technology Stack

Based on crate research:

1. **Core Color System**: `owo-colors` + `anstyle`
   - `owo-colors`: Zero-allocation, built-in environment variable support
   - `anstyle`: Clap compatibility and public API interoperability

2. **Progress Indicators**: `indicatif`
   - Comprehensive spinner and progress bar support
   - Multi-threading compatible
   - Integration with console ecosystem

3. **Text Layout**: `textwrap`
   - Unicode-aware text wrapping
   - Terminal width detection
   - Column alignment support

4. **Terminal Detection**: `std::io::IsTerminal` (Rust 1.70+)
   - Standard library TTY detection
   - No external dependencies

### Architecture Design

#### 1. Theme System

```rust
#[derive(ValueEnum, Clone, Debug)]
pub enum Theme {
    Auto,     // Detect light/dark based on terminal
    Light,    // Light background optimized
    Dark,     // Dark background optimized  
    HighContrast, // Accessibility focused
}

pub struct ColorTheme {
    pub success: anstyle::Color,
    pub error: anstyle::Color,
    pub warning: anstyle::Color,
    pub info: anstyle::Color,
    pub muted: anstyle::Color,
    pub timestamp: anstyle::Color,
    pub resource_id: anstyle::Color,
    pub header: anstyle::Style,
}
```

#### 2. Color Context Manager

```rust
pub struct ColorContext {
    enabled: bool,
    theme: ColorTheme,
    capabilities: TerminalCapabilities,
}

impl ColorContext {
    pub fn new(color_choice: ColorChoice, theme: Theme) -> Self {
        let enabled = match color_choice {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => Self::should_use_color(),
        };
        
        let capabilities = TerminalCapabilities::detect();
        let theme = ColorTheme::for_theme(theme, &capabilities);
        
        Self { enabled, theme, capabilities }
    }
    
    fn should_use_color() -> bool {
        use std::io::IsTerminal;
        
        // Respect NO_COLOR environment variable
        if std::env::var("NO_COLOR").is_ok() {
            return false;
        }
        
        // Check if stdout is a TTY
        std::io::stdout().is_terminal()
    }
}
```

#### 3. Semantic Color Markup

```rust
pub trait ColorExt {
    fn success(self) -> String;
    fn error(self) -> String;
    fn warning(self) -> String;
    fn info(self) -> String;
    fn muted(self) -> String;
    fn timestamp(self) -> String;
    fn resource_id(self) -> String;
    fn header(self) -> String;
}

impl ColorExt for &str {
    fn success(self) -> String {
        GLOBAL_COLOR_CONTEXT.format_success(self)
    }
    // ... other implementations
}
```

#### 4. Terminal Capabilities Detection

```rust
pub struct TerminalCapabilities {
    pub has_color: bool,
    pub has_true_color: bool,
    pub width: Option<usize>,
}

impl TerminalCapabilities {
    pub fn detect() -> Self {
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
```

#### 5. Progress Indication

```rust
pub struct ProgressManager {
    spinner: Option<indicatif::ProgressBar>,
    is_tty: bool,
}

impl ProgressManager {
    pub fn new(context: &ColorContext) -> Self {
        let is_tty = context.enabled && std::io::stdout().is_terminal();
        let spinner = if is_tty {
            Some(indicatif::ProgressBar::new_spinner())
        } else {
            None
        };
        
        Self { spinner, is_tty }
    }
    
    pub fn set_message(&self, msg: &str) {
        if let Some(spinner) = &self.spinner {
            spinner.set_message(msg.to_string());
        } else {
            eprintln!("{}", msg); // Fallback for non-TTY
        }
    }
}
```

### CLI Integration

#### Extended CLI Arguments

```rust
#[derive(Debug, Args)]
#[clap(next_help_heading = "Global Options")]
pub struct GlobalOpts {
    #[arg(long, value_enum, global = true, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
    
    #[arg(long, value_enum, global = true, default_value_t = Theme::Auto)]
    pub theme: Theme,
    
    // ... existing fields
}
```

#### ColorChoice Enhancement

Update ColorChoice::Auto implementation to:
1. Check TTY status using `std::io::IsTerminal`
2. Respect NO_COLOR environment variable
3. Check FORCE_COLOR environment variable

### Implementation Phases

#### Phase 1: Core Infrastructure
1. Add color/theming dependencies to Cargo.toml
2. Implement TerminalCapabilities detection
3. Create ColorContext manager
4. Update ColorChoice::Auto logic

#### Phase 2: Theme System
1. Define ColorTheme struct and default themes
2. Implement semantic color markup traits
3. Add --theme CLI argument
4. Create theme selection logic

#### Phase 3: Progress Indicators
1. Add indicatif dependency
2. Implement ProgressManager
3. Integrate with CloudFormation operations
4. Add spinner support to watch operations

#### Phase 4: Output Formatting
1. Add textwrap dependency for column alignment
2. Implement auto-width adjustment
3. Update existing output code to use semantic markup
4. Add text wrapping for terminal width

#### Phase 5: Integration and Polish
1. Update all output locations to use new color system
2. Add tests for different terminal scenarios
3. Documentation and examples
4. Performance optimization

## Usage Examples

### Basic Usage
```rust
// In CloudFormation operations
println!("{}", "Stack creation started".info());
println!("{}", format!("Stack {} created successfully", stack_name).success());

// Status display with semantic colors
println!("{}", status.colorize_by_status()); // Uses theme colors

// Progress indication
let progress = ProgressManager::new(&color_context);
progress.set_message("Creating stack...");
```

### Theme Configuration
```bash
# Use specific theme
iidy --theme=dark create-stack stack.yaml

# Force color even in non-TTY
iidy --color=always create-stack stack.yaml

# Disable colors completely
iidy --color=never create-stack stack.yaml
```

## Testing Strategy

1. **Unit Tests**: Test color formatting and theme selection
2. **Integration Tests**: Test TTY detection and environment variable handling
3. **Manual Testing**: Test in various terminal environments
4. **CI Testing**: Test with NO_COLOR and different terminal types

## Future Enhancements

1. **Custom Themes**: Support for user-defined theme files
2. **RGB Color Customization**: Allow hex color specification in themes
3. **Terminal Query**: Advanced terminal capability detection
4. **Performance Profiling**: Optimize for high-frequency output scenarios

## Dependencies to Add

```toml
[dependencies]
owo-colors = "4.0"
anstyle = "1.0"  
indicatif = "0.17"
textwrap = "0.16"
terminal_size = "0.3"  # For width detection if not using crossterm
```

Note: `std::io::IsTerminal` is available in Rust 1.70+ so no additional TTY detection dependency needed.