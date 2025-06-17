//! Output renderer trait and implementations
//!
//! This module defines the OutputRenderer trait and specific implementations
//! for different output modes (Interactive, Plain, JSON, TUI).

use crate::output::data::*;
use async_trait::async_trait;
use anyhow::Result;
use clap::ValueEnum;

/// Main trait for rendering output data in different modes
#[async_trait]
pub trait OutputRenderer: Send + Sync {
    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()>;
    async fn render_stack_definition(&mut self, data: &StackDefinition, show_times: bool) -> Result<()>;
    async fn render_stack_events(&mut self, data: &StackEventsDisplay) -> Result<()>;
    async fn render_stack_contents(&mut self, data: &StackContents) -> Result<()>;
    async fn render_status_update(&mut self, data: &StatusUpdate) -> Result<()>;
    async fn render_command_result(&mut self, data: &CommandResult) -> Result<()>;
    async fn render_stack_list(&mut self, data: &StackListDisplay) -> Result<()>;
    async fn render_changeset_result(&mut self, data: &ChangeSetCreationResult) -> Result<()>;
    async fn render_error(&mut self, data: &ErrorInfo) -> Result<()>;
    
    // Control methods
    async fn init(&mut self) -> Result<()>;
    async fn cleanup(&mut self) -> Result<()>;
}

/// Output mode selection
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum OutputMode {
    /// Non-interactive text for CI/logs (no spinners)
    Plain,
    /// Interactive text with spinners and colors (exact iidy-js match)
    Interactive,
    /// Machine-readable JSON Lines format
    Json,
    // TUI mode will be implemented later
}

impl OutputMode {
    pub fn default_for_environment() -> Self {
        use std::io::IsTerminal;
        
        if std::io::stdout().is_terminal() {
            OutputMode::Interactive
        } else {
            OutputMode::Plain
        }
    }
}

impl std::fmt::Display for OutputMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            OutputMode::Plain => "plain",
            OutputMode::Interactive => "interactive",
            OutputMode::Json => "json",
        };
        write!(f, "{}", s)
    }
}