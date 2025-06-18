//! Dynamic output manager for handling mode switching and event buffering
//!
//! This module provides the core DynamicOutputManager that coordinates
//! between different output renderers and manages event history for 
//! seamless mode switching.

use crate::output::data::*;
use crate::output::renderer::{OutputRenderer, OutputMode};
use anyhow::Result;
use std::collections::VecDeque;

/// Options for configuring output behavior
#[derive(Debug, Clone)]
pub struct OutputOptions {
    pub color_choice: crate::cli::ColorChoice,
    pub theme: crate::cli::Theme,
    pub terminal_width: Option<usize>,
    pub buffer_limit: usize,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            color_choice: crate::cli::ColorChoice::Auto,
            theme: crate::cli::Theme::Auto,
            terminal_width: None,
            buffer_limit: 1000, // Keep last 1000 events for mode switching
        }
    }
}

/// Dynamic output manager that handles mode switching and event replay
pub struct DynamicOutputManager {
    current_mode: OutputMode,
    current_renderer: Box<dyn OutputRenderer>,
    event_buffer: VecDeque<OutputData>,
    options: OutputOptions,
}

impl DynamicOutputManager {
    /// Create a new dynamic output manager
    pub async fn new(mode: OutputMode, options: OutputOptions) -> Result<Self> {
        let mut renderer = create_renderer(mode, &options)?;
        renderer.init().await?;
        
        Ok(Self {
            current_mode: mode,
            current_renderer: renderer,
            event_buffer: VecDeque::with_capacity(options.buffer_limit),
            options,
        })
    }
    
    /// Render data with the current renderer and buffer for mode switching
    pub async fn render(&mut self, data: OutputData) -> Result<()> {
        // Buffer the data for mode switching replay
        if self.event_buffer.len() >= self.options.buffer_limit {
            self.event_buffer.pop_front();
        }
        self.event_buffer.push_back(data.clone());
        
        // Render with current mode
        self.render_data(&data).await
    }
    
    /// Switch to a different output mode
    pub async fn switch_to_mode(&mut self, new_mode: OutputMode) -> Result<()> {
        if new_mode == self.current_mode {
            return Ok(());
        }
        
        // Clean up current renderer
        self.current_renderer.cleanup().await?;
        
        // Clear screen logic will be added when TUI is implemented
        
        // Create new renderer
        self.current_renderer = create_renderer(new_mode, &self.options)?;
        self.current_renderer.init().await?;
        
        // Re-render all buffered data in new mode
        let buffered_data: Vec<OutputData> = self.event_buffer.iter().cloned().collect();
        for data in buffered_data {
            self.render_data(&data).await?;
        }
        
        self.current_mode = new_mode;
        
        // Show switch notification
        let switch_msg = StatusUpdate {
            message: format!("Switched to {} mode", new_mode),
            timestamp: chrono::Utc::now(),
            level: crate::output::data::StatusLevel::Info,
        };
        self.render_data(&OutputData::StatusUpdate(switch_msg)).await?;
        
        Ok(())
    }
    
    /// Get current output mode
    pub fn current_mode(&self) -> OutputMode {
        self.current_mode
    }
    
    /// Clear the event buffer
    pub fn clear_buffer(&mut self) {
        self.event_buffer.clear();
    }
    
    /// Get the number of buffered events
    pub fn buffer_len(&self) -> usize {
        self.event_buffer.len()
    }
    
    /// Internal method to render a single data item
    async fn render_data(&mut self, data: &OutputData) -> Result<()> {
        match data {
            OutputData::CommandMetadata(metadata) => {
                self.current_renderer.render_command_metadata(metadata).await
            }
            OutputData::StackDefinition(def, show_times) => {
                self.current_renderer.render_stack_definition(def, *show_times).await
            }
            OutputData::StackEvents(events) => {
                self.current_renderer.render_stack_events(events).await
            }
            OutputData::StackContents(contents) => {
                self.current_renderer.render_stack_contents(contents).await
            }
            OutputData::StatusUpdate(update) => {
                self.current_renderer.render_status_update(update).await
            }
            OutputData::CommandResult(result) => {
                self.current_renderer.render_command_result(result).await
            }
            OutputData::StackList(list) => {
                self.current_renderer.render_stack_list(list).await
            }
            OutputData::ChangeSetResult(result) => {
                self.current_renderer.render_changeset_result(result).await
            }
            OutputData::StackDrift(drift) => {
                self.current_renderer.render_stack_drift(drift).await
            }
            OutputData::Error(error) => {
                self.current_renderer.render_error(error).await
            }
            OutputData::TokenInfo(token) => {
                self.current_renderer.render_token_info(token).await
            }
        }
    }
}

/// Create a renderer for the specified mode
fn create_renderer(mode: OutputMode, options: &OutputOptions) -> Result<Box<dyn OutputRenderer>> {
    match mode {
        OutputMode::Plain => {
            let plain_options = crate::output::renderers::plain::PlainTextOptions {
                show_timestamps: true,
                max_line_width: options.terminal_width,
            };
            Ok(Box::new(crate::output::renderers::plain::PlainTextRenderer::new(plain_options)))
        }
        OutputMode::Interactive => {
            let interactive_options = crate::output::renderers::interactive::InteractiveOptions {
                theme: options.theme,
                color_choice: options.color_choice,
                terminal_width: options.terminal_width,
                show_timestamps: true,
            };
            Ok(Box::new(crate::output::renderers::interactive::InteractiveRenderer::new(interactive_options)))
        }
        OutputMode::Json => {
            let json_options = crate::output::renderers::json::JsonOptions {
                include_timestamps: true,
                pretty_print: false, // JSONL format should be compact
                include_type: true,
            };
            Ok(Box::new(crate::output::renderers::json::JsonRenderer::new(json_options)))
        }
    }
}