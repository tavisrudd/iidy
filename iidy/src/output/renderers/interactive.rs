//! Interactive renderer with exact iidy-js formatting
//!
//! This renderer provides pixel-perfect output matching the original iidy-js implementation,
//! including colors, spacing, timestamps, and all formatting details.

use crate::output::data::*;
use crate::output::renderer::OutputRenderer;
use crate::output::theme::{IidyTheme, get_terminal_width};
use crate::cli::{Theme, ColorChoice};
use crate::color::{ProgressManager, SpinnerStyle};
use crate::cfn::CfnOperation;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::io::{self, Write, IsTerminal};
use std::collections::VecDeque;
use std::time::Duration;
use std::sync::{Arc, Mutex};

// Core constants matching iidy-js exactly (from complete implementation spec)
pub const COLUMN2_START: usize = 25;
// Removed DEFAULT_STATUS_PADDING as it's not used in this exact implementation
pub const MIN_STATUS_PADDING: usize = 17;
pub const MAX_PADDING: usize = 60;
pub const RESOURCE_TYPE_PADDING: usize = 40;

// Live events timing constants
const LIVE_EVENTS_UPDATE_INTERVAL_SECS: u64 = 1;


/// Configuration options for interactive rendering
#[derive(Debug, Clone)]
pub struct InteractiveOptions {
    pub theme: Theme,
    pub color_choice: ColorChoice,
    pub terminal_width: Option<usize>,
    pub show_timestamps: bool,
    pub enable_spinners: bool,
    pub enable_ansi_features: bool,
    pub cli_context: Option<Arc<crate::cli::Cli>>,
}

impl Default for InteractiveOptions {
    fn default() -> Self {
        Self {
            theme: Theme::Auto,
            color_choice: ColorChoice::Auto,
            terminal_width: None, // Will auto-detect
            show_timestamps: true,
            enable_spinners: true,
            enable_ansi_features: true,
            cli_context: None,
        }
    }
}

impl InteractiveOptions {
    /// Create options for plain text mode (no colors, spinners, or ANSI features)
    pub fn plain() -> Self {
        Self {
            theme: Theme::Auto, // Doesn't matter since colors are disabled
            color_choice: ColorChoice::Never,
            terminal_width: None,
            show_timestamps: true,
            enable_spinners: false,
            enable_ansi_features: false,
            cli_context: None,
        }
    }
}

/// Interactive renderer with exact iidy-js formatting and colors
pub struct InteractiveRenderer {
    options: InteractiveOptions,
    theme: IidyTheme,
    #[allow(dead_code)] // Will be used for text wrapping in future
    terminal_width: usize,
    has_rendered_content: bool,
    // Async ordering state
    current_operation: Option<String>,
    expected_sections: Vec<&'static str>,
    pending_sections: std::collections::HashMap<&'static str, OutputData>,
    current_spinner: Option<ProgressManager>,
    next_section_index: usize,
    suppress_main_heading: bool,
    printed_sections: Vec<String>, // Track which section titles have been printed
    cli_context: Option<Arc<crate::cli::Cli>>,
    // Section titles configured during construction
    section_titles: HashMap<&'static str, String>,
    // Live events timing state (local to spinner)
    timing_task_handle: Option<tokio::task::JoinHandle<()>>,
    timing_state: Option<Arc<Mutex<(DateTime<Utc>, Option<DateTime<Utc>>)>>>, // (start_time, last_live_event_time)
    // Buffer for live events that arrive before we're ready to display them
    buffered_live_events: Vec<OutputData>,
}

impl InteractiveRenderer {
    pub fn new(options: InteractiveOptions) -> Self {
        let theme = IidyTheme::new(options.theme, options.color_choice);
        let terminal_width = options.terminal_width.unwrap_or_else(get_terminal_width);
        let cli_context = options.cli_context.clone();
        
        let mut renderer = Self { 
            options,
            theme,
            terminal_width,
            has_rendered_content: false,
            current_operation: None,
            expected_sections: Vec::new(),
            pending_sections: std::collections::HashMap::new(),
            current_spinner: None,
            next_section_index: 0,
            suppress_main_heading: false,
            printed_sections: Vec::new(),
            cli_context: cli_context.clone(),
            section_titles: HashMap::new(),
            timing_task_handle: None,
            timing_state: None,
            buffered_live_events: Vec::new(),
        };
        
        // Set up operation context if CLI context is available
        if let Some(ref cli) = cli_context {
            let operation = cli.command.to_cfn_operation();
            renderer.setup_operation(&operation, cli);
        }
        
        renderer
    }
    
    /// Check if colors are enabled
    fn colors_enabled(&self) -> bool {
        self.theme.colors_enabled
    }
    
    /// Format section heading (exact iidy-js implementation)
    fn format_section_heading(&self, text: &str) -> String {
        // Remove trailing colon if present to avoid double colons
        let clean_text = text.trim_end_matches(':');
        
        if self.colors_enabled() {
            format!("{}:", clean_text.color(self.theme.section_heading).bold())
        } else {
            format!("{}:", clean_text)
        }
    }
    
    /// Print section heading with appropriate spacing (without trailing newline)
    fn print_section_heading(&mut self, text: &str) {
        // Add blank line before section if any content has been rendered
        if self.has_rendered_content {
            println!();
        }
        print!("{}", self.format_section_heading(text));
        self.printed_sections.push(text.to_string());
        self.has_rendered_content = true; // Keep this for other spacing logic
    }
    
    /// Print section heading with newline (for sections that need content on separate lines)
    fn print_section_heading_with_newline(&mut self, text: &str) {
        self.print_section_heading(text);
        println!();
    }
    
    /// Add appropriate spacing before content if needed
    fn add_content_spacing(&mut self) {
        if self.has_rendered_content {
            println!();
        }
        self.has_rendered_content = true;
    }
    
    /// Format section label (exact iidy-js implementation) 
    fn format_section_label(&self, text: &str) -> String {
        if self.colors_enabled() {
            text.color(self.theme.muted).to_string() // iidy-js: truecolor(128, 128, 128) - blackBright for section labels
        } else {
            text.to_string()
        }
    }
    
    /// Format section entry (exact iidy-js implementation)
    fn format_section_entry(&self, label: &str, data: &str) -> String {
        format!(" {}{}\n", 
            self.format_section_label(&format!("{:<width$} ", label, width = COLUMN2_START - 1)),
            data
        )
    }
    
    /// Print section entry to stdout (exact iidy-js implementation)
    fn print_section_entry(&self, label: &str, data: &str) -> Result<()> {
        print!("{}", self.format_section_entry(label, data));
        io::stdout().flush()?;
        Ok(())
    }
    
    /// Format logical ID (exact iidy-js implementation)
    fn format_logical_id(&self, text: &str) -> String {
        if self.colors_enabled() {
            text.color(self.theme.resource_id).to_string() // iidy-js: xterm color 252 - light gray for logical resource IDs
        } else {
            text.to_string()
        }
    }
    
    /// Format timestamp (exact iidy-js implementation)
    fn format_timestamp(&self, text: &str) -> String {
        if self.colors_enabled() {
            text.color(self.theme.timestamp).to_string() // iidy-js: xterm color 253 - light gray for timestamps
        } else {
            text.to_string()
        }
    }
    
    /// Render timestamp in iidy-js format (canonical format for all timestamps)
    fn render_timestamp(&self, dt: &DateTime<Utc>) -> String {
        dt.format("%a %b %d %Y %H:%M:%S").to_string()
    }
    
    /// Render timestamp for stack events in iidy-js format
    fn render_event_timestamp(&self, dt: &DateTime<Utc>) -> String {
        self.render_timestamp(dt)
    }
    
    /// Colorize resource status (exact iidy-js implementation)
    fn colorize_resource_status(&self, status: &str, padding: Option<usize>) -> String {
        if !self.colors_enabled() {
            return match padding {
                Some(width) => format!("{:<width$}", status, width = width),
                None => status.to_string(),
            };
        }
        
        let colored_status = if status.contains("IN_PROGRESS") {
            status.color(self.theme.warning).to_string() // iidy-js: yellow for IN_PROGRESS states
        } else if status.contains("COMPLETE") {
            status.color(self.theme.success).to_string() // iidy-js: green for COMPLETE states
        } else if status.contains("FAILED") {
            status.color(self.theme.error).to_string() // iidy-js: red for FAILED states
        } else if status == "DELETE_SKIPPED" {
            status.color(self.theme.skipped).to_string() // iidy-js: xterm color 240 - dark gray for DELETE_SKIPPED
        } else {
            status.color(self.theme.info).to_string() // iidy-js: white for other states
        };
        
        match padding {
            Some(width) => format!("{:<width$}", colored_status, width = width),
            None => colored_status,
        }
    }
    
    /// Calculate padding for a collection of items (exact iidy-js implementation)
    fn calc_padding<T, F>(&self, items: &[T], extractor: F) -> usize 
    where
        F: Fn(&T) -> &str,
    {
        let max_len = items.iter()
            .map(|item| extractor(item).len())
            .max()
            .unwrap_or(0);
        
        std::cmp::min(std::cmp::max(max_len, MIN_STATUS_PADDING), MAX_PADDING)
    }
    
    /// Pretty format tags (exact iidy-js implementation)
    fn pretty_format_tags(&self, tags: &HashMap<String, String>) -> String {
        self.pretty_format_tags_with_truncation(tags, None)
    }
    
    /// Pretty format tags with optional truncation and Environment tag prioritization
    fn pretty_format_tags_with_truncation(&self, tags: &HashMap<String, String>, max_tags: Option<usize>) -> String {
        if tags.is_empty() {
            return String::new();
        }
        
        let mut formatted_tags = Vec::new();
        
        // First, add Environment/environment tag if it exists (case-insensitive)
        let env_keys = ["Environment", "environment", "ENVIRONMENT", "env", "ENV"];
        for env_key in &env_keys {
            if let Some(env_value) = tags.get(*env_key) {
                formatted_tags.push(format!("{}={}", env_key, env_value));
                break; // Only add the first match
            }
        }
        
        // Then add other tags (excluding the environment tag we already added)
        let mut other_tags: Vec<String> = tags.iter()
            .filter(|(key, _)| !env_keys.contains(&key.as_str()))
            .map(|(key, value)| format!("{}={}", key, value))
            .collect();
        other_tags.sort();
        
        // Apply truncation if specified
        if let Some(max_tags) = max_tags {
            let remaining_slots = max_tags.saturating_sub(formatted_tags.len());
            if remaining_slots < other_tags.len() {
                other_tags.truncate(remaining_slots.saturating_sub(1)); // Leave room for "..."
                other_tags.push("...".to_string());
            }
        }
        
        formatted_tags.extend(other_tags);
        formatted_tags.join(", ")
    }
    
    /// Pretty format parameters (exact iidy-js implementation)
    fn pretty_format_parameters(&self, params: &HashMap<String, String>) -> String {
        if params.is_empty() {
            return String::new();
        }
        
        let mut formatted_params: Vec<String> = params.iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect();
        formatted_params.sort();
        formatted_params.join(", ")
    }
    
    /// Pretty format small map (exact iidy-js implementation)
    fn pretty_format_small_map(&self, map: &HashMap<String, String>) -> String {
        if map.is_empty() {
            return String::new();
        }
        
        let mut items: Vec<String> = map.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        items.sort();
        items.join(", ")
    }
    
    // Color methods removed - using theme.field.color() directly for clarity
    
    /// Apply environment-based coloring (exact iidy-js implementation)
    fn color_by_environment(&self, text: &str, env_name: &str) -> String {
        if !self.colors_enabled() {
            return text.to_string();
        }
        
        match env_name {
            "production" => text.color(self.theme.env_production).to_string(), // iidy-js: red for production environments
            "integration" => text.color(self.theme.env_integration).to_string(), // iidy-js: xterm color 75 - blue-ish for integration
            "development" => text.color(self.theme.env_development).to_string(), // iidy-js: xterm color 194 - yellow-ish for development
            _ => text.to_string(),
        }
    }
    
    /// Format token source (exact iidy-js implementation)
    fn format_token_source(&self, source: &TokenSource) -> String {
        match source {
            TokenSource::UserProvided => "user-provided".to_string(),
            TokenSource::AutoGenerated => "auto-generated".to_string(),
            TokenSource::Derived { from, step } => format!("derived from {} at {}", from, step),
        }
    }
    
    /// Create spinner for API waiting periods (iidy-js style) - only in TTY and if enabled
    fn create_api_spinner(&self, message: &str) -> Option<ProgressManager> {
        if self.options.enable_spinners && self.colors_enabled() && io::stdout().is_terminal() {
            // Style spinner text with muted color (brightblack) like iidy-js
            let styled_message = self.style_muted_text(message);
            Some(ProgressManager::with_style(SpinnerStyle::Dots12, &styled_message))
        } else {
            // Spinners disabled, non-TTY, or colors disabled: just print the message
            println!("{}", message);
            None
        }
    }
    
    /// Format timing text for live events (helper method)
    fn format_timing_text(total_elapsed: i64, last_live_event: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
        if let Some(last_event_time) = last_live_event {
            let since_last_event = (now - last_event_time).num_seconds();
            format!("{} seconds elapsed total. {} since last event.", 
                total_elapsed, since_last_event)
        } else {
            format!("{} seconds elapsed total.", total_elapsed)
        }
    }
    
    /// Apply muted theme styling to text (helper method)
    fn style_muted_text(&self, text: &str) -> String {
        text.color(self.theme.muted).to_string()
    }
    
    /// Start live events timing with background updates (like iidy-js setInterval)
    fn start_live_events_timing(&mut self, start_time: DateTime<Utc>) {
        // Only start if we have an active spinner and spinners are enabled
        if let Some(spinner) = &self.current_spinner {
            if let Some(spinner_ref) = spinner.get_spinner_ref() {
                // Create shared state for timing info (start_time, last_live_event_time)
                // Start with None for last_live_event_time since we haven't seen any live events yet
                let timing_state = Arc::new(Mutex::new((start_time, None)));
                let timing_ref = Arc::clone(&timing_state);
                
                // Get theme color for styling
                let muted_color = self.theme.muted;
                
                // Spawn background task that updates spinner every second
                let task = tokio::spawn(async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(LIVE_EVENTS_UPDATE_INTERVAL_SECS));
                    
                    loop {
                        interval.tick().await;
                        
                        let timing_guard = timing_ref.lock().unwrap();
                        let (start, last_live_event): (DateTime<Utc>, Option<DateTime<Utc>>) = *timing_guard;
                        drop(timing_guard); // Release the lock
                        
                        let now = Utc::now();
                        let total_elapsed = (now - start).num_seconds();
                        
                        // Format and style timing text
                        let timing_text = Self::format_timing_text(total_elapsed, last_live_event, now);
                        let styled_text = timing_text.color(muted_color).to_string();
                        
                        // Update spinner message
                        spinner_ref.set_message(styled_text);
                    }
                });
                
                // Store task handle and timing state
                self.timing_task_handle = Some(task);
                self.timing_state = Some(timing_state);
            }
        }
    }
    
    /// Update last event time when new live events arrive
    fn update_last_event_time(&mut self, event_time: DateTime<Utc>) {
        if let Some(timing_state) = &self.timing_state {
            if let Ok(mut state) = timing_state.lock() {
                state.1 = Some(event_time); // Update last_live_event_time
            }
        }
    }
    
    /// Stop live events timing and clean up
    fn stop_live_events_timing(&mut self) {
        if let Some(handle) = self.timing_task_handle.take() {
            handle.abort();
        }
        self.timing_state = None;
    }
    
}

#[async_trait]
impl OutputRenderer for InteractiveRenderer {
    async fn init(&mut self) -> Result<()> {
        // Operation setup already done in new()
        Ok(())
    }
    
    async fn cleanup(&mut self) -> Result<()> {
        // Flush any remaining output
        io::stdout().flush()?;
        Ok(())
    }
    
    /// Render OutputData with ordering logic for parallel operations
    async fn render_output_data(&mut self, data: OutputData, buffer: Option<&VecDeque<OutputData>>) -> Result<()> {
        // If we have an active operation with expected sections, handle ordering
        if !self.expected_sections.is_empty() {
            return self.render_with_ordering(data, buffer).await;
        }
        
        // No operation context, render immediately
        self.render_data_immediately(data).await
    }
}

impl InteractiveRenderer {
    /// Setup operation context and configure section titles based on CLI arguments
    fn setup_operation(&mut self, operation: &CfnOperation, cli: &crate::cli::Cli) {
        self.current_operation = Some(operation.to_string());
        
        // Configure section titles based on operation and CLI arguments
        self.configure_section_titles(operation, cli);
        
        // Define expected sections based on operation for async ordering
        self.expected_sections = match operation {
            // Read-only operations - focus on data display
            CfnOperation::DescribeStack => vec!["stack_definition", "stack_events", "stack_contents"],
            CfnOperation::ListStacks => vec!["stack_list"],
            CfnOperation::WatchStack => vec!["stack_definition", "stack_events", "live_stack_events", "stack_contents"],
            CfnOperation::GetStackTemplate => vec![], // Exception: doesn't use renderer system
            CfnOperation::DescribeStackDrift => vec!["stack_drift"],
            
            // Modification operations with monitoring (include command_metadata as first section)
            CfnOperation::CreateStack => vec!["command_metadata", "stack_definition", "live_stack_events", "stack_contents"],
            CfnOperation::DeleteStack => vec!["command_metadata", "stack_definition", "stack_events", "stack_contents", "live_stack_events"],
            CfnOperation::UpdateStack => vec!["command_metadata", "stack_definition", "live_stack_events", "stack_contents"],
            CfnOperation::CreateChangeset => vec!["changeset_result"],
            CfnOperation::ExecuteChangeset => vec!["command_metadata", "live_stack_events", "stack_contents"],
            CfnOperation::CreateOrUpdate => vec!["command_metadata", "stack_definition", "live_stack_events", "stack_contents"],
            
            // Other operations
            CfnOperation::EstimateCost => vec!["cost_estimate"],
            CfnOperation::GetStackInstances => vec!["stack_instances"],
        };
        
        // Start the first section for operations with predefined sections (show title immediately, spinner if enabled)
        // Exception: GetStackTemplate doesn't use the renderer system
        if !matches!(*operation, CfnOperation::GetStackTemplate) {
            self.start_next_section();
        }
    }
    
    /// Start the next section (show title immediately, spinner if enabled)
    fn start_next_section(&mut self) -> () {
        if self.next_section_index < self.expected_sections.len() {
            let section_key = self.expected_sections[self.next_section_index];
            let title = self.get_section_title(section_key).to_string();
            
            // Show section heading immediately (with newline if section is always multi-line)
            if self.section_is_always_multiline(section_key) {
                self.print_section_heading_with_newline(&title);
            } else {
                self.print_section_heading(&title);
            }
            
            if self.options.enable_spinners {
                // Always put spinner on the line after the heading
                if !self.section_is_always_multiline(section_key) {
                    println!(); // Add newline after inline heading for spinner
                }
                self.current_spinner = self.create_api_spinner(&format!("Loading {}...", title.to_lowercase()));
                
                // Special handling for live_stack_events section - start timing immediately
                if section_key == "live_stack_events" {
                    let start_time = Utc::now();
                    self.start_live_events_timing(start_time);
                }
            }
        }
    }
    
    /// Configure section titles based on operation and CLI arguments
    fn configure_section_titles(&mut self, operation: &CfnOperation, cli: &crate::cli::Cli) {
        use crate::cli::Commands;
        
        // Default titles (operation-specific titles will override these)
        self.section_titles.insert("stack_definition", "Stack Details".to_string());
        self.section_titles.insert("stack_list", "Stack List".to_string());
        self.section_titles.insert("stack_drift", "Stack Drift".to_string());
        self.section_titles.insert("changeset_result", "Changeset Result".to_string());
        self.section_titles.insert("stack_contents", "Stack Resources".to_string());
        self.section_titles.insert("stack_events", "Stack Events".to_string());
        self.section_titles.insert("command_metadata", "Command Metadata".to_string());
        self.section_titles.insert("live_stack_events", "Live Stack Events".to_string());
        
        // Configure stack events title based on operation
        match operation {
            CfnOperation::DescribeStack => {
                if let Commands::DescribeStack(args) = &cli.command {
                    self.section_titles.insert("stack_events", 
                        format!("Previous Stack Events (max {}):", args.events));
                }
            }
            CfnOperation::WatchStack => {
                // For watch-stack, we have separate previous and live events sections
                self.section_titles.insert("stack_events", "Previous Stack Events (max 10):".to_string());
                self.section_titles.insert("live_stack_events", "Live Stack Events (2s poll):".to_string());
            }
            CfnOperation::CreateStack => {
                // For create-stack, include command metadata and live events
                self.section_titles.insert("command_metadata", "Command Metadata:".to_string());
                self.section_titles.insert("stack_definition", "Stack Details".to_string());
                self.section_titles.insert("stack_contents", "Stack Resources".to_string());
                self.section_titles.insert("live_stack_events", "Live Stack Events (2s poll):".to_string());
            }
            CfnOperation::DeleteStack => {
                // For delete-stack, include command metadata, previous events, and live events  
                self.section_titles.insert("command_metadata", "Command Metadata:".to_string());
                self.section_titles.insert("stack_definition", "Stack Details".to_string());
                self.section_titles.insert("stack_events", "Previous Stack Events (max 10):".to_string());
                self.section_titles.insert("stack_contents", "Stack Resources".to_string());
                self.section_titles.insert("live_stack_events", "Live Stack Events (2s poll):".to_string());
            }
            _ => {
                self.section_titles.insert("stack_events", "Stack Events:".to_string());
            }
        }
    }
    
    /// Get the section title for display
    fn get_section_title(&self, section_key: &str) -> &str {
        self.section_titles.get(section_key).map(|s| s.as_str()).unwrap_or("Loading")
    }
    
    /// Check if a section should always be multi-line (needs newline after heading)
    fn section_is_always_multiline(&self, section_key: &str) -> bool {
        match section_key {
            "command_metadata" => true,      // Command Metadata is always multi-line
            "stack_definition" => true,      // Stack Details is always multi-line
            "stack_contents" => true,        // Stack Resources is always multi-line
            "stack_events" => true,          // Previous Stack Events is always multi-line
            "live_stack_events" => true,     // Live Stack Events is always multi-line
            _ => false,
        }
    }
    
    /// Render data with ordering logic for parallel operations
    async fn render_with_ordering(&mut self, data: OutputData, _buffer: Option<&VecDeque<OutputData>>) -> Result<()> {
        let section_key = self.get_section_key(&data);
        
        if let Some(key) = section_key {
            if key == "live_stack_events" {
                // Special case: streaming section
                self.handle_live_events_data(data).await?;
            } else {
                // Regular case: static section
                self.pending_sections.insert(key, data);
                self.advance_through_ready_sections().await?;
            }
        } else {
            // Non-section data (TokenInfo, OperationComplete, etc.)
            self.handle_non_section_data(data).await?;
        }
        
        Ok(())
    }
    
    /// Advance through sections that have data ready
    async fn advance_through_ready_sections(&mut self) -> Result<()> {
        while self.next_section_index < self.expected_sections.len() {
            let section_key = self.expected_sections[self.next_section_index];
            
            if let Some(data) = self.pending_sections.remove(section_key) {
                self.render_section(data).await?;
                self.next_section_index += 1;
                self.start_next_section_if_exists();
            } else {
                break; // Wait for this section's data
            }
        }
        Ok(())
    }

    /// Render a single section with its data
    async fn render_section(&mut self, data: OutputData) -> Result<()> {
        // Clear current spinner
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }
        
        // Set flag to suppress main section heading (already shown)
        self.suppress_main_heading = true;
        
        // Render content (main heading will be suppressed)
        self.render_data_immediately(data).await?;
        
        // Reset flag
        self.suppress_main_heading = false;
        
        Ok(())
    }

    /// Start next section if it exists
    fn start_next_section_if_exists(&mut self) {
        if self.next_section_index < self.expected_sections.len() {
            self.start_next_section();
        } else {
            self.cleanup_operation();
        }
    }
    
    /// Flush any buffered live events
    async fn flush_buffered_live_events(&mut self) -> Result<()> {
        let events_to_render: Vec<OutputData> = self.buffered_live_events.drain(..).collect();
        for buffered_event in events_to_render {
            self.render_data_immediately(buffered_event).await?;
        }
        Ok(())
    }

    /// Handle live events data (special streaming section)
    async fn handle_live_events_data(&mut self, data: OutputData) -> Result<()> {
        // Advance through any ready sections first
        self.advance_through_ready_sections().await?;
        
        // Find the index of live_stack_events in expected sections
        let live_events_index = self.expected_sections.iter()
            .position(|&section| section == "live_stack_events");
        
        if let Some(target_index) = live_events_index {
            // If we haven't reached the live_stack_events section yet, buffer the data
            if self.next_section_index < target_index {
                self.buffered_live_events.push(data);
                return Ok(());
            }
            
            // We're at the live_stack_events section
            if self.next_section_index == target_index {
                // Start live_stack_events section if not already started
                if !self.section_already_started("live_stack_events") {
                    self.start_next_section();
                    
                    // Render any buffered events first
                    self.flush_buffered_live_events().await?;
                }
                
                // Render the current event immediately (streaming)
                self.render_data_immediately(data).await?;
            }
        }
        
        Ok(())
    }

    /// Handle non-section data
    async fn handle_non_section_data(&mut self, data: OutputData) -> Result<()> {
        match data {
            OutputData::OperationComplete(ref info) => {
                // Advance past live_stack_events if we're there
                if self.next_section_index < self.expected_sections.len() && 
                   self.expected_sections[self.next_section_index] == "live_stack_events" {
                    self.next_section_index += 1;
                }
                
                if info.skip_remaining_sections {
                    self.cleanup_operation();
                } else {
                    self.advance_through_ready_sections().await?;
                }
            },
            _ => {
                self.render_data_immediately(data).await?;
            }
        }
        Ok(())
    }

    /// Check if a section has already been started
    fn section_already_started(&self, section_key: &str) -> bool {
        let section_title = self.get_section_title(section_key);
        self.printed_sections.contains(&section_title.to_string())
    }
    
    /// Get section key for OutputData type
    fn get_section_key(&self, data: &OutputData) -> Option<&'static str> {
        match data {
            OutputData::CommandMetadata(..) => Some("command_metadata"),
            OutputData::StackDefinition(..) => Some("stack_definition"),
            OutputData::StackEvents(..) => Some("stack_events"),
            OutputData::StackContents(..) => Some("stack_contents"),
            OutputData::StackList(..) => Some("stack_list"),
            OutputData::StackDrift(..) => Some("stack_drift"),
            OutputData::ChangeSetResult(..) => Some("changeset_result"),
            OutputData::NewStackEvents(..) => Some("live_stack_events"),
            // Only non-section data returns None
            OutputData::TokenInfo(..) | OutputData::OperationComplete(..) | OutputData::InactivityTimeout(..) => None,
            _ => None,
        }
    }
    

    /// Clean up operation state
    fn cleanup_operation(&mut self) -> () {
        // Clear any remaining spinner
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }
        
        self.current_operation = None;
        self.expected_sections.clear();
        self.pending_sections.clear();
        self.buffered_live_events.clear();
        self.next_section_index = 0;
        self.suppress_main_heading = false;
        self.printed_sections.clear();
    }
    
    /// Render data immediately without ordering logic
    async fn render_data_immediately(&mut self, data: OutputData) -> Result<()> {
        match data {
            OutputData::CommandMetadata(ref metadata) => self.render_command_metadata(metadata).await,
            OutputData::StackDefinition(ref def, show_times) => self.render_stack_definition(def, show_times).await,
            OutputData::StackEvents(ref events) => self.render_stack_events(events).await,
            OutputData::StackContents(ref contents) => self.render_stack_contents(contents).await,
            OutputData::StatusUpdate(ref update) => self.render_status_update(update).await,
            OutputData::CommandResult(ref result) => self.render_command_result(result).await,
            OutputData::FinalCommandSummary(ref summary) => self.render_final_command_summary(summary).await,
            OutputData::StackList(ref list) => self.render_stack_list(list).await,
            OutputData::ChangeSetResult(ref result) => self.render_changeset_result(result).await,
            OutputData::StackDrift(ref drift) => self.render_stack_drift(drift).await,
            OutputData::Error(ref error) => self.render_error(error).await,
            OutputData::TokenInfo(ref token) => self.render_token_info(token).await,
            OutputData::NewStackEvents(ref events) => self.render_new_stack_events(events).await,
            OutputData::OperationComplete(ref info) => self.handle_operation_complete(info).await,
            OutputData::InactivityTimeout(ref info) => self.handle_inactivity_timeout(info).await,
        }
    }
    
}

impl InteractiveRenderer {
    /// Render command metadata (exact iidy-js showCommandSummary implementation)
    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()> {
        // Print section heading unless suppressed (section ordering system handles this)
        if !self.suppress_main_heading {
            self.print_section_heading_with_newline("Command Metadata:");
        }
        
        // Get CFN operation from CLI context
        if let Some(ref cli) = self.cli_context {
            let operation = cli.command.to_cfn_operation();
            self.print_section_entry("CFN Operation:", &operation.to_string().color(self.theme.primary).to_string())?;
        }
        self.print_section_entry("iidy Environment:", &data.iidy_environment.color(self.theme.primary).to_string())?;
        self.print_section_entry("Region:", &data.region.color(self.theme.primary).to_string())?;
        
        if let Some(profile) = &data.profile {
            if !profile.is_empty() {
                self.print_section_entry("Profile:", &profile.color(self.theme.primary).to_string())?;
            }
        }
        
        let cli_args = self.pretty_format_small_map(&data.cli_arguments);
        self.print_section_entry("CLI Arguments:", &cli_args.color(self.theme.muted).to_string())?;
        
        let service_role = data.iam_service_role.as_deref().unwrap_or("None");
        self.print_section_entry("IAM Service Role:", &service_role.color(self.theme.muted).to_string())?;
        
        self.print_section_entry("Current IAM Principal:", &data.current_iam_principal.color(self.theme.muted).to_string())?;
        self.print_section_entry("iidy Version:", &data.iidy_version.color(self.theme.muted).to_string())?;
        
        // Client Request Token (following iidy-js pattern)
        self.print_section_entry("Client Req Token:", &format!("{} ({})", 
            data.primary_token.value.color(self.theme.muted), 
            self.format_token_source(&data.primary_token.source)
        ))?;
        
        // Derived Tokens (following iidy-js pattern) - only show if present
        if !data.derived_tokens.is_empty() {
            self.print_section_entry("Derived Tokens:", &format!("{} tokens", data.derived_tokens.len()))?;
            for (i, token) in data.derived_tokens.iter().enumerate() {
                self.print_section_entry(&format!("  [{}]", i + 1), &format!("{} ({})", 
                    token.value.color(self.theme.muted),
                    self.format_token_source(&token.source)
                ))?;
            }
        }
        
        Ok(())
    }
    
    /// Render stack definition (exact iidy-js summarizeStackDefinition implementation)
    async fn render_stack_definition(&mut self, data: &StackDefinition, show_times: bool) -> Result<()> {
        if !self.suppress_main_heading {
            self.print_section_heading_with_newline("Stack Details");
        }
        // Note: When suppress_main_heading is true, the heading was already printed with newline
        // by the spinner logic since Stack Details is always multi-line
        
        // Handle StackSet name display
        if let Some(stackset_name) = data.tags.get("StackSetName") {
            self.print_section_entry("Name (StackSet):", 
                &format!("{} {}", 
                    data.name.color(self.theme.muted),
                    stackset_name.color(self.theme.primary)
                )
            )?;
        } else {
            self.print_section_entry("Name:", &data.name.color(self.theme.primary).to_string())?;
        }
        
        // Description
        if let Some(description) = &data.description {
            let description_color = if data.name.starts_with("StackSet") {
                description.color(self.theme.primary).to_string()
            } else {
                description.color(self.theme.muted).to_string()
            };
            self.print_section_entry("Description:", &description_color)?;
        }
        
        // Status
        self.print_section_entry("Status:", &self.colorize_resource_status(&data.status, None))?;
        
        // Capabilities
        let capabilities = if data.capabilities.is_empty() {
            "None".to_string()
        } else {
            data.capabilities.join(", ")
        };
        self.print_section_entry("Capabilities:", &capabilities.color(self.theme.muted).to_string())?;
        
        // Service Role
        let service_role = data.service_role.as_deref().unwrap_or("None");
        self.print_section_entry("Service Role:", &service_role.color(self.theme.muted).to_string())?;
        
        // Tags
        let tags_str = self.pretty_format_tags(&data.tags);
        self.print_section_entry("Tags:", &tags_str.color(self.theme.muted).to_string())?;
        
        // Parameters
        let params_str = self.pretty_format_parameters(&data.parameters);
        self.print_section_entry("Parameters:", &params_str.color(self.theme.muted).to_string())?;
        
        // DisableRollback
        self.print_section_entry("DisableRollback:", &data.disable_rollback.to_string().color(self.theme.muted).to_string())?;
        
        // TerminationProtection
        let protection_text = format!("{}{}", 
            data.termination_protection.to_string().color(self.theme.muted),
            if data.termination_protection { " 🔒" } else { "" }
        );
        self.print_section_entry("TerminationProtection:", &protection_text)?;
        
        // Times (conditional)
        if show_times {
            if let Some(creation_time) = &data.creation_time {
                self.print_section_entry("Creation Time:", 
                    &self.render_timestamp(creation_time).color(self.theme.muted).to_string())?;
            }
            if let Some(last_updated_time) = &data.last_updated_time {
                self.print_section_entry("Last Update Time:", 
                    &self.render_timestamp(last_updated_time).color(self.theme.muted).to_string())?;
            }
        }
        
        // Timeout
        if let Some(timeout) = data.timeout_in_minutes {
            self.print_section_entry("Timeout In Minutes:", 
                &timeout.to_string().color(self.theme.muted).to_string())?;
        }
        
        // NotificationARNs
        let notification_arns = if data.notification_arns.is_empty() {
            "None".to_string()
        } else {
            data.notification_arns.join(", ")
        };
        self.print_section_entry("NotificationARNs:", &notification_arns.color(self.theme.muted).to_string())?;
        
        // Stack Policy
        if let Some(policy) = &data.stack_policy {
            self.print_section_entry("Stack Policy Source:", &policy.color(self.theme.muted).to_string())?;
        }
        
        // ARN
        self.print_section_entry("ARN:", &data.arn.color(self.theme.muted).to_string())?;
        
        // Console URL
        self.print_section_entry("Console URL:", &data.console_url.color(self.theme.muted).to_string())?;
        
        Ok(())
    }
    
    /// Render stack events (exact iidy-js implementation)
    async fn render_stack_events(&mut self, data: &StackEventsDisplay) -> Result<()> {
        // Section heading already shown with correct title during start_next_section()
        // No need to rewrite it with ANSI escapes
        
        if data.events.is_empty() {
            // Only show "No events found" for previous events, not for live events title-only display
            if !data.title.contains("Live Stack Events") {
                println!(" {}", "No events found".color(self.theme.muted));
            }
            return Ok(());
        }
        
        // Sort events and apply limiting using the helper method
        let (events_to_show, truncation_info) = data.get_sorted_limited_events();
        
        // Calculate padding for status and resource type columns
        let status_padding = self.calc_padding(&events_to_show, |e| &e.event.resource_status);
        let resource_type_padding = self.calc_padding(&events_to_show, |e| &e.event.resource_type);
        
        for event_with_timing in &events_to_show {
            let event = &event_with_timing.event;
            
            // Format timestamp (iidy-js format: "Sun Jul 10 2016 14:00:12")
            let timestamp = if let Some(ts) = &event.timestamp {
                self.format_timestamp(&self.render_event_timestamp(ts))
            } else {
                self.format_timestamp("                         ")
            };
            
            // Format status with padding (apply padding before coloring to avoid ANSI length issues)
            let status_padded = format!("{:<width$}", event.resource_status, width = status_padding);
            let status = if self.colors_enabled() {
                self.colorize_resource_status(&status_padded, None)
            } else {
                status_padded
            };
            
            // Format resource type with padding (plain white for events, not muted)
            let resource_type_padded = format!("{:<width$}", 
                event.resource_type, 
                width = resource_type_padding
            );
            let resource_type = if self.colors_enabled() {
                resource_type_padded.color(self.theme.info).to_string() // Use info color (white) for events
            } else {
                resource_type_padded
            };
            
            // Format logical ID (no padding needed as it's the last column before duration)
            let logical_id = self.format_logical_id(&event.logical_resource_id);
            
            // Format duration if available
            let duration_text = if let Some(duration) = event_with_timing.duration_seconds {
                format!(" ({}s)", duration)
            } else {
                String::new()
            };
            
            // iidy-js column order: timestamp status resource_type logical_id duration
            println!(" {} {} {} {}{}",
                timestamp,
                status,
                resource_type,
                logical_id,
                duration_text.color(self.theme.muted)
            );
        }
        
        // Show truncation info if present
        if let Some(truncation) = &truncation_info {
            println!("  {}", format!(
                "showing {} of {} events", 
                truncation.shown, 
                truncation.total
            ).color(self.theme.muted));
        }
        
        Ok(())
    }
    
    /// Render stack contents (exact iidy-js summarizeStackContents implementation)
    async fn render_stack_contents(&mut self, data: &StackContents) -> Result<()> {
        // TODO fix the handling of titles in here.
        // Stack Resources
        if !data.resources.is_empty() {
            // Only show heading if not suppressed (i.e., not called from ordering logic)
            if !self.suppress_main_heading {
                self.print_section_heading_with_newline("Stack Resources");
            }
            // Note: When suppress_main_heading is true, the heading was already printed with newline
            // by the spinner logic since Stack Resources is always multi-line
            let id_padding = self.calc_padding(&data.resources, |r| &r.logical_resource_id);
            let resource_type_padding = self.calc_padding(&data.resources, |r| &r.resource_type);
            
            for resource in &data.resources {
                println!("{} {} {}",
                    self.format_logical_id(&format!(" {:<width$}", 
                        resource.logical_resource_id, 
                        width = id_padding
                    )),
                    format!("{:<width$}", 
                        resource.resource_type, 
                        width = resource_type_padding
                    ).color(self.theme.muted),
                    resource.physical_resource_id.as_deref().unwrap_or("").color(self.theme.muted)
                );
            }
        }
        
        // Stack Outputs
        if data.outputs.is_empty() {
            self.print_section_heading("Stack Outputs");
            println!(" {}", "None".color(self.theme.muted));
        } else {
            self.print_section_heading_with_newline("Stack Outputs");
            let output_key_padding = self.calc_padding(&data.outputs, |o| &o.output_key);
            
            for output in &data.outputs {
                println!("{} {}",
                    self.format_logical_id(&format!(" {:<width$}", 
                        output.output_key, 
                        width = output_key_padding
                    )),
                    output.output_value.color(self.theme.muted)
                );
            }
        }
        
        // Stack Exports
        if !data.exports.is_empty() {
            // Check if we have importing stacks or multiple exports - if so, use multi-line
            let has_imports = data.exports.iter().any(|export| !export.importing_stacks.is_empty());
            let is_complex = data.exports.len() > 1 || has_imports;
            
            if is_complex {
                self.print_section_heading_with_newline("Stack Exports");
            } else {
                self.print_section_heading("Stack Exports");
            }
            
            let export_name_padding = self.calc_padding(&data.exports, |ex| &ex.name);
            
            for export in &data.exports {
                println!("{} {}",
                    self.format_logical_id(&format!(" {:<width$}", 
                        export.name, 
                        width = export_name_padding
                    )),
                    export.value.color(self.theme.muted)
                );
                
                // Show imports
                for import in &export.importing_stacks {
                    println!("  {}", format!("imported by {}", import).color(self.theme.muted));
                }
            }
        }
        
        // Current Stack Status
        self.print_section_heading("Current Stack Status");
        println!(" {} {}",
            self.colorize_resource_status(&data.current_status.status, None),
            data.current_status.status_reason.as_deref().unwrap_or("").color(self.theme.muted)
        );
        
        // Pending Changesets
        if !data.pending_changesets.is_empty() {
            self.print_section_heading_with_newline("Pending Changesets");
            
            for changeset in &data.pending_changesets {
                self.print_section_entry(
                    &self.format_timestamp(&if let Some(ct) = &changeset.creation_time {
                        self.render_timestamp(ct)
                    } else {
                        "Unknown".to_string()
                    }),
                    &format!("{} {} {}",
                        changeset.change_set_name.color(self.theme.primary),
                        changeset.status,
                        changeset.status_reason.as_deref().unwrap_or("").color(self.theme.muted)
                    )
                )?;
                
                if let Some(description) = &changeset.description {
                    if !description.is_empty() {
                        println!("  Description: {}", description.color(self.theme.muted));
                        println!();
                    }
                }
                
                // Show changeset changes
                for change in &changeset.changes {
                    println!("    {} {} {}",
                        self.format_logical_id(&change.logical_resource_id),
                        self.colorize_resource_status(&change.action, None),
                        change.resource_type.color(self.theme.muted)
                    );
                    
                    if let Some(replacement) = &change.replacement {
                        println!("      {}", format!("Replacement: {}", replacement).color(self.theme.muted));
                    }
                    
                    for detail in &change.details {
                        println!("      {}", format!("{}: {}", detail.target, detail.change_source.as_deref().unwrap_or("Unknown")).color(self.theme.muted));
                    }
                }
                
                println!();
            }
        }
        
        Ok(())
    }
    
    /// Render status update (exact iidy-js implementation)
    async fn render_status_update(&mut self, data: &StatusUpdate) -> Result<()> {
        let timestamp = if self.options.show_timestamps {
            format!("{} ", self.format_timestamp(&self.render_timestamp(&data.timestamp)))
        } else {
            String::new()
        };
        
        let message = match data.level {
            StatusLevel::Error => data.message.color(self.theme.error).to_string(),
            StatusLevel::Warning => if self.colors_enabled() { 
                data.message.color(self.theme.warning).to_string()
            } else { 
                data.message.to_string() 
            },
            StatusLevel::Info => data.message.to_string(),
            StatusLevel::Success => data.message.color(self.theme.success).to_string(),
        };
        
        println!("{}{}", timestamp, message);
        Ok(())
    }
    
    /// Render command result (exact iidy-js implementation)
    async fn render_command_result(&mut self, data: &CommandResult) -> Result<()> {
        self.add_content_spacing();
        
        let status_text = if data.success {
            self.format_section_heading("SUCCESS")
        } else {
            format!("{}:", "FAILURE".color(self.theme.error))
        };
        
        println!("{} ({}s)", status_text, data.elapsed_seconds);
        
        if let Some(message) = &data.message {
            println!("{}", message);
        }
        
        Ok(())
    }
    
    /// Render final command summary (exact iidy-js showFinalComandSummary implementation)
    async fn render_final_command_summary(&mut self, data: &crate::output::data::FinalCommandSummary) -> Result<()> {
        use crate::output::data::CommandSummaryResult;
        
        // Add spacing before final summary
        self.add_content_spacing();
        
        // Print section heading (exact iidy-js: "Command Summary:" with column2_start padding)
        let summary_text = match data.result {
            CommandSummaryResult::Success => {
                if self.colors_enabled() {
                    // Success with green background and thumbs up emoji (exact iidy-js pattern)
                    format!("{} 👍", "Success".on_green().black())
                } else {
                    "Success 👍".to_string()
                }
            },
            CommandSummaryResult::Failure => {
                if self.colors_enabled() {
                    // Failure with red background and table flip emoji (exact iidy-js pattern)
                    format!("{} (╯°□°）╯︵ ┻━┻", "Failure".on_red().white())
                } else {
                    "Failure (╯°□°）╯︵ ┻━┻".to_string()
                }
            }
        };
        self.print_section_entry("Command Summary:", &summary_text)?;
        
        // Show "Fix and try again" message for failures (exact iidy-js pattern)
        if matches!(data.result, CommandSummaryResult::Failure) {
            println!("Fix and try again.");
        }
        
        Ok(())
    }
    
    /// Render stack list (exact iidy-js listStacks implementation)
    async fn render_stack_list(&mut self, data: &StackListDisplay) -> Result<()> {
        if data.stacks.is_empty() {
            println!("No stacks found");
            return Ok(());
        }
        
        // Calculate padding for proper column alignment (exact iidy-js constants)
        const TIME_PADDING: usize = 24; // Fixed padding like iidy-js
        let status_padding = self.calc_padding(&data.stacks, |s| &s.stack_status);
        
        // Header with exact spacing to match column alignment
        let header = if data.show_tags {
            format!("{:<width1$} {:<width2$} Name, Tags", 
                "Creation/Update Time,", 
                "Status,",
                width1 = TIME_PADDING,
                width2 = status_padding)
        } else {
            format!("{:<width1$} {:<width2$} Name", 
                "Creation/Update Time,", 
                "Status,",
                width1 = TIME_PADDING,
                width2 = status_padding)
        };
        println!("{}", header.color(self.theme.muted));
        
        for stack in &data.stacks {
            // Lifecycle icons
            let lifecycle_icon = if stack.termination_protection || stack.tags.get("lifetime") == Some(&"protected".to_string()) {
                "🔒 "
            } else if stack.tags.get("lifetime") == Some(&"long".to_string()) {
                "∞ "
            } else if stack.tags.get("lifetime") == Some(&"short".to_string()) {
                "♺ "
            } else {
                ""
            };
            
            // Base stack name (handle StackSet)
            let base_stack_name = if stack.stack_name.starts_with("StackSet-") {
                format!("{} {}", 
                    stack.stack_name.color(self.theme.muted).to_string(),
                    stack.tags.get("StackSetName")
                        .unwrap_or(&"Unknown stack set instance".to_string())
                )
            } else {
                stack.stack_name.clone()
            };
            
            // Environment-based coloring
            let env_name = if stack.stack_name.contains("production") || stack.tags.get("environment") == Some(&"production".to_string()) {
                "production"
            } else if stack.stack_name.contains("integration") || stack.tags.get("environment") == Some(&"integration".to_string()) {
                "integration"
            } else if stack.stack_name.contains("development") || stack.tags.get("environment") == Some(&"development".to_string()) {
                "development"
            } else {
                ""
            };
            
            let stack_name = self.color_by_environment(&base_stack_name, env_name);
            
            // Format timestamp with exact padding (iidy-js format)
            let timestamp = if let Some(time) = &stack.last_updated_time {
                format!("{:>width$}", self.render_event_timestamp(time), width = TIME_PADDING)
            } else if let Some(time) = &stack.creation_time {
                format!("{:>width$}", self.render_event_timestamp(time), width = TIME_PADDING)
            } else {
                format!("{:>width$}", "Unknown", width = TIME_PADDING)
            };
            
            let tags_display = if data.show_tags {
                // Use truncated tags for list view - show Environment tag first and limit to 3 total tags
                format!(" {}", self.pretty_format_tags_with_truncation(&stack.tags, Some(3)).color(self.theme.muted))
            } else {
                String::new()
            };
            
            // Format status with proper padding (avoid ANSI color issues)
            let status_padded = format!("{:<width$}", stack.stack_status, width = status_padding);
            let status_colored = if self.colors_enabled() {
                self.colorize_resource_status(&status_padded, None)
            } else {
                status_padded
            };
            
            println!("{} {} {}{}{}",
                self.format_timestamp(&timestamp),
                status_colored,
                lifecycle_icon.color(self.theme.muted),
                stack_name,
                tags_display
            );
            
            // Failure reason on next line
            if stack.stack_status.contains("FAILED") {
                if let Some(reason) = &stack.status_reason {
                    if !reason.is_empty() {
                        println!("   {}", reason.color(self.theme.muted));
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Render changeset result (exact iidy-js implementation)
    async fn render_changeset_result(&mut self, data: &ChangeSetCreationResult) -> Result<()> {
        self.add_content_spacing();
        
        // Show AWS Console URL
        println!("AWS Console URL for full changeset review: {}",
            data.console_url.color(self.theme.muted));
        
        // Show status
        let status_text = if data.has_changes {
            format!("Changeset {} {} with changes",
                data.changeset_name.magenta(),
                data.status
            )
        } else {
            format!("Changeset {} {} with no changes",
                data.changeset_name.magenta(),
                data.status
            )
        };
        
        println!("{}", status_text);
        
        Ok(())
    }
    
    /// Render stack drift (exact iidy-js implementation)
    async fn render_stack_drift(&mut self, data: &StackDrift) -> Result<()> {
        if data.drifted_resources.is_empty() {
            println!("No drift detected. Stack resources are in sync with template.");
        } else {
            self.print_section_heading_with_newline("Drifted Resources");
            
            // Calculate padding for aligned output (similar to iidy-js calcPadding)
            let id_padding = data.drifted_resources.iter()
                .map(|d| d.logical_resource_id.len())
                .max()
                .unwrap_or(0);
            let type_padding = data.drifted_resources.iter()
                .map(|d| d.resource_type.len())
                .max()
                .unwrap_or(0);
                
            for drift in &data.drifted_resources {
                // Following iidy-js formatting pattern 
                println!(" {:<width1$} {:<width2$} {}", 
                    drift.logical_resource_id.color(self.theme.resource_id),
                    drift.resource_type.color(self.theme.muted),
                    drift.physical_resource_id.color(self.theme.muted),
                    width1 = id_padding,
                    width2 = type_padding
                );
                println!("  {}", drift.drift_status.color(self.theme.error));
                
                if !drift.property_differences.is_empty() {
                    // Format as YAML with indentation (matching iidy-js pattern)
                    for diff in &drift.property_differences {
                        println!("   - property_path: {}", diff.property_path);
                        if let Some(expected) = &diff.expected_value {
                            println!("     expected_value: {}", expected);
                        }
                        if let Some(actual) = &diff.actual_value {
                            println!("     actual_value: {}", actual);
                        }
                        if let Some(diff_type) = &diff.difference_type {
                            println!("     difference_type: {}", diff_type);
                        }
                    }
                }
            }
        }
        println!();
        Ok(())
    }
    
    /// Render error (exact iidy-js implementation)
    async fn render_error(&mut self, data: &ErrorInfo) -> Result<()> {
        eprintln!("{}: {}", 
            "Error".color(self.theme.error), 
            data.message
        );
        
        if let Some(details) = &data.details {
            eprintln!("{}", details.color(self.theme.muted));
        }
        
        Ok(())
    }
    
    /// Render token info - typically only shown in debug/verbose modes
    async fn render_token_info(&mut self, data: &TokenInfo) -> Result<()> {
        // In interactive mode, only show tokens in debug scenarios
        // For now, we'll keep it simple and not display by default
        // TODO: Add verbosity/debug flag checking
        let _ = data; // Suppress unused parameter warning
        Ok(())
    }
    
    /// Render new stack events (batch of events for live watch - no title/header)
    async fn render_new_stack_events(&mut self, events: &[StackEventWithTiming]) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        
        // Preserve timing state before clearing spinner
        let preserved_start_time = if let Some(timing_state) = &self.timing_state {
            timing_state.lock().ok().map(|state| state.0)
        } else {
            None
        };
        
        // Stop timing task and clear spinner for clean event output
        self.stop_live_events_timing();
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }
        
        // Update last event time with the most recent event
        if let Some(latest_event) = events.last() {
            if let Some(_event_time) = latest_event.event.timestamp {
                // We'll restore timing after creating new spinner, so just track the event time
            }
        }
        
        // Calculate padding for status and resource type columns (same as stack events)
        let status_padding = self.calc_padding(events, |e| &e.event.resource_status);
        let resource_type_padding = self.calc_padding(events, |e| &e.event.resource_type);
        
        for event_with_timing in events {
            let event = &event_with_timing.event;
            
            // Format timestamp (iidy-js format: "Sun Jul 10 2016 14:00:12")
            let timestamp = if let Some(ts) = &event.timestamp {
                self.format_timestamp(&self.render_event_timestamp(ts))
            } else {
                self.format_timestamp("                         ")
            };
            
            // Format status with padding (apply padding before coloring to avoid ANSI length issues)
            let status_padded = format!("{:<width$}", event.resource_status, width = status_padding);
            let status = if self.colors_enabled() {
                self.colorize_resource_status(&status_padded, None)
            } else {
                status_padded
            };
            
            // Format resource type with padding (plain white for events, not muted)
            let resource_type_padded = format!("{:<width$}", 
                event.resource_type, 
                width = resource_type_padding
            );
            let resource_type = if self.colors_enabled() {
                resource_type_padded.color(self.theme.info).to_string() // Use info color (white) for events
            } else {
                resource_type_padded
            };
            
            // Format logical ID (no padding needed as it's the last column before duration)
            let logical_id = self.format_logical_id(&event.logical_resource_id);
            
            // Format duration if available
            let duration_text = if let Some(duration) = event_with_timing.duration_seconds {
                format!(" ({}s)", duration)
            } else {
                String::new()
            };
            
            // iidy-js column order: timestamp status resource_type logical_id duration
            println!(" {} {} {} {}{}",
                timestamp,
                status,
                resource_type,
                logical_id,
                duration_text.color(self.theme.muted)
            );
        }
        
        // Restart the spinner AND timing task for continued live events polling
        if self.options.enable_spinners && self.current_spinner.is_none() {
            // Create new spinner
            self.current_spinner = self.create_api_spinner("Loading live events...");
            
            // Restart timing task with preserved start time
            if let Some(start_time) = preserved_start_time {
                self.start_live_events_timing(start_time);
                
                // Update last event time with the most recent event from this batch
                if let Some(latest_event) = events.last() {
                    if let Some(event_time) = latest_event.event.timestamp {
                        self.update_last_event_time(event_time);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Handle operation completion (control flow only - no display)
    async fn handle_operation_complete(&mut self, info: &OperationCompleteInfo) -> Result<()> {
        // Stop live events timing task
        self.stop_live_events_timing();
        
        // Clear any active spinner (live events spinner)
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }
        
        // Show final elapsed time message (like iidy-js line 89)
        let message = format!(" {} seconds elapsed total.", info.elapsed_seconds);
        println!("{}", self.style_muted_text(&message));
        
        // This signals that live events are done - advance to next section (unless we should skip)
        if !info.skip_remaining_sections {
            self.advance_to_next_section();
        }
        
        Ok(())
    }
    
    /// Handle inactivity timeout (show message and signal completion)
    async fn handle_inactivity_timeout(&mut self, info: &InactivityTimeoutInfo) -> Result<()> {
        // Stop live events timing task
        self.stop_live_events_timing();
        
        // Clear any active spinner (live events spinner)
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }
        
        // Show timeout message (properly indented like other status messages)
        let timeout_message = format!(" Inactivity timeout of {} seconds reached. Stopping watch.", 
            info.timeout_seconds);
        println!("{}", self.style_muted_text(&timeout_message));
        
        // This signals that live events are done - advance to next section
        self.advance_to_next_section();
        
        Ok(())
    }
    
    /// Advance to the next section (non-recursive)
    fn advance_to_next_section(&mut self) {
        // Move to next section
        if self.next_section_index < self.expected_sections.len() {
            self.next_section_index += 1;
            
            // Start next section (title + spinner if enabled)
            self.start_next_section();
        }
    }
    
    
}
