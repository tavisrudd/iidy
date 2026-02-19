//! Data-driven output architecture for iidy
//!
//! This module implements a clean separation between data collection and presentation,
//! enabling multiple output modes (Interactive, Plain, JSON, TUI) with exact iidy-js
//! compatibility in Interactive mode.

pub mod aws_conversion;
pub mod color;
pub mod data;
pub mod fixtures;
pub mod manager;
pub mod renderer;
pub mod renderers;
pub mod spinner;
pub mod terminal;
pub mod theme;

// Re-exports for convenience
pub use aws_conversion::*;
pub use color::*;
pub use data::*;
pub use fixtures::FixtureLoader;
pub use manager::DynamicOutputManager;
pub use renderer::{OutputMode, OutputRenderer};
pub use renderers::*;
pub use spinner::*;
pub use terminal::*;
