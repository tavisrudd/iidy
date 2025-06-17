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

// Re-exports for convenience
pub use data::*;
pub use renderer::{OutputRenderer, OutputMode};
pub use renderers::*;
pub use manager::DynamicOutputManager;
pub use fixtures::FixtureLoader;