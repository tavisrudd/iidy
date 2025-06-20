//! Dynamic output manager for handling mode switching and event buffering
//!
//! This module provides the core DynamicOutputManager that coordinates
//! between different output renderers and manages event history for 
//! seamless mode switching.

use crate::output::data::*;
use crate::output::renderer::{OutputRenderer, OutputMode};
use crate::cli::Cli;
use anyhow::Result;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::mpsc::{self, UnboundedSender, UnboundedReceiver};

/// Options for configuring output behavior
#[derive(Debug, Clone)]
pub struct OutputOptions {
    pub color_choice: crate::cli::ColorChoice,
    pub theme: crate::cli::Theme,
    pub terminal_width: Option<usize>,
    pub buffer_limit: usize,
    pub cli_context: Arc<Cli>,
}

impl OutputOptions {
    /// Create new output options with required CLI context
    pub fn new(cli_context: Cli) -> Self {
        Self {
            color_choice: cli_context.global_opts.color,
            theme: cli_context.global_opts.theme,
            terminal_width: None,
            buffer_limit: 1000, // Keep last 1000 events for mode switching
            cli_context: Arc::new(cli_context),
        }
    }
    
    /// Create minimal options for stub/incomplete implementations
    /// TODO: Remove this once all command handlers are updated to pass full CLI context
    pub fn minimal() -> Self {
        use crate::cli::{Commands, DescribeArgs, GlobalOpts, AwsOpts, ColorChoice, Theme};
        let cli = Cli {
            global_opts: GlobalOpts {
                environment: "development".to_string(),
                color: ColorChoice::Auto,
                theme: Theme::Auto,
                output_mode: None,
                debug: false,
                log_full_error: false,
            },
            aws_opts: AwsOpts {
                region: None,
                profile: None,
                assume_role_arn: None,
                client_request_token: None,
            },
            command: Commands::DescribeStack(DescribeArgs {
                stackname: "stub".to_string(),
                events: 50,
                query: None,
            }),
        };
        Self::new(cli)
    }
}

/// Dynamic output manager that handles mode switching and event replay
pub struct DynamicOutputManager {
    current_mode: OutputMode,
    current_renderer: Box<dyn OutputRenderer>,
    event_buffer: VecDeque<OutputData>,
    buffer_limit: usize,
    // Parallel rendering channel
    parallel_receiver: Option<UnboundedReceiver<OutputData>>,
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
            buffer_limit: options.buffer_limit,
            parallel_receiver: None,
        })
    }
    
    /// Render data with the current renderer and buffer for mode switching
    pub async fn render(&mut self, data: OutputData) -> Result<()> {
        // Buffer the data for mode switching replay (arrival order)
        if self.event_buffer.len() >= self.buffer_limit {
            self.event_buffer.pop_front();
        }
        self.event_buffer.push_back(data.clone());
        
        // Render with current mode, passing buffer reference for ordering
        self.current_renderer.render_output_data(data, Some(&self.event_buffer)).await
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
        // TODO: We need to store options to recreate renderer. For now, create with minimal options.
        let temp_options = OutputOptions::minimal();
        self.current_renderer = create_renderer(new_mode, &temp_options)?;
        self.current_renderer.init().await?;
        
        // Re-render all buffered data in new mode
        let buffered_data: Vec<OutputData> = self.event_buffer.iter().cloned().collect();
        for data in buffered_data {
            self.current_renderer.render_output_data(data, Some(&self.event_buffer)).await?;
        }
        
        self.current_mode = new_mode;
        
        // Show switch notification
        let switch_msg = StatusUpdate {
            message: format!("Switched to {} mode", new_mode),
            timestamp: chrono::Utc::now(),
            level: crate::output::data::StatusLevel::Info,
        };
        self.current_renderer.render_output_data(OutputData::StatusUpdate(switch_msg), Some(&self.event_buffer)).await?;
        
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
    
    /// Start parallel rendering mode and return a sender for OutputData
    /// 
    /// The caller should:
    /// 1. Spawn tasks that send OutputData through the channel
    /// 2. Drop the sender when done spawning
    /// 3. Call `stop()` to process and render all data
    pub fn start(&mut self) -> UnboundedSender<OutputData> {
        let (tx, rx) = mpsc::unbounded_channel::<OutputData>();
        self.parallel_receiver = Some(rx);
        tx
    }
    
    /// Process and render all data from parallel operations
    /// 
    /// Collects all parallel data and renders in arrival order.
    /// Renderers can handle their own ordering logic if needed.
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut rx) = self.parallel_receiver.take() {
            // Render data as it arrives (arrival order)
            while let Some(data) = rx.recv().await {
                self.render(data).await?;
            }
        }
        Ok(())
    }
    
}

/// Create a renderer for the specified mode
fn create_renderer(mode: OutputMode, options: &OutputOptions) -> Result<Box<dyn OutputRenderer>> {
    match mode {
        OutputMode::Plain => {
            // Use InteractiveRenderer with plain configuration
            let interactive_options = crate::output::renderers::interactive::InteractiveOptions {
                theme: options.theme, // Theme doesn't matter since colors are disabled
                color_choice: crate::cli::ColorChoice::Never, // Force no colors
                terminal_width: options.terminal_width,
                show_timestamps: true,
                enable_spinners: false, // No spinners in plain mode
                enable_ansi_features: false, // No ANSI features in plain mode
                cli_context: Some(options.cli_context.clone()), // Pass CLI context for proper ordering
            };
            Ok(Box::new(crate::output::renderers::interactive::InteractiveRenderer::new(interactive_options)))
        }
        OutputMode::Interactive => {
            let interactive_options = crate::output::renderers::interactive::InteractiveOptions {
                theme: options.theme,
                color_choice: options.color_choice,
                terminal_width: options.terminal_width,
                show_timestamps: true,
                enable_spinners: true,
                enable_ansi_features: true,
                cli_context: Some(options.cli_context.clone()),
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