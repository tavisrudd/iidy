//! Data-driven output architecture for iidy
//!
//! This module implements a clean separation between data collection and presentation,
//! enabling multiple output modes (Interactive, Plain, JSON, TUI) with exact iidy-js
//! compatibility in Interactive mode.

pub mod data;
pub mod renderer;
pub mod renderers;
pub mod manager;
pub mod fixtures;
pub mod theme;
pub mod keyboard;
pub mod aws_conversion;
pub mod color;
pub mod terminal;

// Re-exports for convenience
pub use data::*;
pub use renderer::{OutputRenderer, OutputMode};
pub use renderers::*;
pub use manager::DynamicOutputManager;
pub use fixtures::FixtureLoader;
pub use keyboard::{KeyboardListener, KeyboardCommand, KeyboardConfig, is_tty_environment, create_for_environment, handle_keyboard_commands};
pub use aws_conversion::*;
pub use color::*;
pub use terminal::*;