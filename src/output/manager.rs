//! Dynamic output manager for handling mode switching and event buffering
//!
//! This module provides the core DynamicOutputManager that coordinates
//! between different output renderers and manages event history for
//! seamless mode switching.
use crate::cli::Cli;
use crate::output::data::*;
use crate::output::renderer::{OutputMode, OutputRenderer};
use anyhow::Result;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::oneshot;

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
}

/// Dynamic output manager that handles mode switching and event replay
pub struct DynamicOutputManager {
    current_mode: OutputMode,
    current_renderer: Box<dyn OutputRenderer>,
    event_buffer: VecDeque<OutputData>,
    buffer_limit: usize,
    options: OutputOptions, // Store options for mode switching
}

impl DynamicOutputManager {
    /// Create a new dynamic output manager
    pub async fn new(mode: OutputMode, options: OutputOptions) -> Result<Self> {
        let mut renderer = create_renderer(mode, &options)?;
        renderer.init().await?;

        let buffer_limit = options.buffer_limit;

        Ok(Self {
            current_mode: mode,
            current_renderer: renderer,
            event_buffer: VecDeque::with_capacity(buffer_limit),
            buffer_limit,
            options,
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
        self.current_renderer
            .render_output_data(data, Some(&self.event_buffer))
            .await
    }

    /// Request user confirmation and return whether user confirmed
    pub async fn request_confirmation(&mut self, message: String) -> Result<bool> {
        self.request_confirmation_impl(message, None).await
    }

    /// Request user confirmation with a specific section key
    pub async fn request_confirmation_with_key(
        &mut self,
        message: String,
        key: String,
    ) -> Result<bool> {
        self.request_confirmation_impl(message, Some(key)).await
    }

    /// Internal implementation for confirmation requests
    async fn request_confirmation_impl(
        &mut self,
        message: String,
        key: Option<String>,
    ) -> Result<bool> {
        // Create oneshot channel internally
        let (response_tx, response_rx) = oneshot::channel();

        // Create confirmation request with channel
        let confirmation = OutputData::ConfirmationPrompt(ConfirmationRequest {
            message,
            response_tx: Some(response_tx),
            key,
        });

        // Send through normal rendering system (integrates with sections)
        self.render(confirmation).await?;

        // Wait for response from renderer
        response_rx
            .await
            .map_err(|_| anyhow::anyhow!("Confirmation response channel closed"))
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
                enable_spinners: false,      // No spinners in plain mode
                enable_ansi_features: false, // No ANSI features in plain mode
                cli_context: Some(options.cli_context.clone()), // Pass CLI context for proper ordering
            };
            Ok(Box::new(
                crate::output::renderers::interactive::InteractiveRenderer::new(
                    interactive_options,
                ),
            ))
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
            Ok(Box::new(
                crate::output::renderers::interactive::InteractiveRenderer::new(
                    interactive_options,
                ),
            ))
        }
        OutputMode::Json => {
            let json_options = crate::output::renderers::json::JsonOptions {
                include_timestamps: true,
                pretty_print: false, // JSONL format should be compact
                include_type: true,
            };
            Ok(Box::new(crate::output::renderers::json::JsonRenderer::new(
                json_options,
            )))
        }
    }
}
