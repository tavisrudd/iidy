//! Interactive renderer with exact iidy-js formatting
//!
//! This renderer provides pixel-perfect output matching the original iidy-js implementation,
//! including colors, spacing, timestamps, and all formatting details.

use crate::output::data::*;
use crate::output::renderer::OutputRenderer;
use crate::output::theme::{IidyTheme, get_terminal_width};
use crate::cli::{Theme, ColorChoice};
use crate::color::{ProgressManager, SpinnerStyle};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::io::{self, Write, IsTerminal};
use std::collections::VecDeque;
use std::sync::Arc;

// Core constants matching iidy-js exactly (from complete implementation spec)
pub const COLUMN2_START: usize = 25;
// Removed DEFAULT_STATUS_PADDING as it's not used in this exact implementation
pub const MIN_STATUS_PADDING: usize = 17;
pub const MAX_PADDING: usize = 60;
pub const RESOURCE_TYPE_PADDING: usize = 40;

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
        // Add blank line before section if other sections have been printed
        if !self.printed_sections.is_empty() {
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
        if tags.is_empty() {
            return String::new();
        }
        
        let mut formatted_tags: Vec<String> = tags.iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect();
        formatted_tags.sort();
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
            Some(ProgressManager::with_style(SpinnerStyle::Dots12, message))
        } else {
            // Spinners disabled, non-TTY, or colors disabled: just print the message
            println!("{}", message);
            None
        }
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
        // Check if this is CommandMetadata to set up operation context
        if let OutputData::CommandMetadata(ref metadata) = data {
            // Get operation from CLI context
            if let Some(ref cli) = self.cli_context {
                // TODO move this check to init.
                let operation = cli.command.to_cfn_operation();
                if self.should_show_command_metadata(&operation) {
                    self.render_command_metadata(metadata).await?;
                }
            }
            return Ok(());
        }
        
        // If we have an active operation with expected sections, handle ordering
        if !self.expected_sections.is_empty() {
            return self.render_with_ordering(data, buffer).await;
        }
        
        // No operation context, render immediately
        self.render_data_immediately(data).await
    }
}

impl InteractiveRenderer {
    /// Determine if command metadata should be shown for this operation
    fn should_show_command_metadata(&self, operation: &CfnOperation) -> bool {
        !operation.is_read_only()
    }
    
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
            CfnOperation::WatchStack => vec!["stack_definition", "stack_events"],
            CfnOperation::GetStackTemplate => vec![], // Just returns template content
            CfnOperation::DescribeStackDrift => vec!["stack_drift"],
            
            // Modification operations - may have different ordering needs
            CfnOperation::CreateStack | 
            CfnOperation::UpdateStack | 
            CfnOperation::DeleteStack => vec![], // Real-time progress, no predefined sections
            CfnOperation::CreateChangeset => vec!["changeset_result"],
            CfnOperation::ExecuteChangeset => vec![], // Real-time progress
            CfnOperation::CreateOrUpdate => vec![], // Real-time progress
            
            // Other operations
            CfnOperation::EstimateCost => vec![],
            CfnOperation::GetStackInstances => vec![],
        };
        
        // Start the first section for describe-stack (show title immediately, spinner if enabled)
        if *operation == CfnOperation::DescribeStack {
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
            }
        }
    }
    
    /// Configure section titles based on operation and CLI arguments
    fn configure_section_titles(&mut self, operation: &CfnOperation, cli: &crate::cli::Cli) {
        use crate::cli::Commands;
        
        // Default titles
        self.section_titles.insert("stack_definition", "Stack Details".to_string());
        self.section_titles.insert("stack_list", "Stack List".to_string());
        self.section_titles.insert("stack_drift", "Stack Drift".to_string());
        self.section_titles.insert("changeset_result", "Changeset Result".to_string());
        self.section_titles.insert("stack_contents", "Stack Resources".to_string()); // First title for multi-section
        
        // Configure stack events title based on operation
        match operation {
            CfnOperation::DescribeStack => {
                if let Commands::DescribeStack(args) = &cli.command {
                    self.section_titles.insert("stack_events", 
                        format!("Previous Stack Events (max {}):", args.events));
                }
            }
            CfnOperation::WatchStack => {
                self.section_titles.insert("stack_events", "Live Stack Events:".to_string());
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
            "stack_definition" => true,  // Stack Details is always multi-line
            "stack_contents" => true,    // Stack Resources is always multi-line
            "stack_events" => true,      // Stack Events is always multi-line
            _ => false,
        }
    }
    
    /// Render data with ordering logic for parallel operations
    async fn render_with_ordering(&mut self, data: OutputData, _buffer: Option<&VecDeque<OutputData>>) -> Result<()> {
        let section_key = self.get_section_key(&data);
        
        // Store the data for this section
        if let Some(key) = section_key {
            self.pending_sections.insert(key, data);
            
            // Render sections in expected order as they become available
            self.render_available_sections().await?;
        } else {
            // Not a section we're tracking, render immediately
            self.render_data_immediately(data).await?;
        }
        
        Ok(())
    }
    
    /// Render all available sections in expected order
    async fn render_available_sections(&mut self) -> Result<()> {
        // Render sections starting from the current position
        while self.next_section_index < self.expected_sections.len() {
            let section_key = self.expected_sections[self.next_section_index];
            
            if let Some(data) = self.pending_sections.remove(section_key) {
                // Clear current spinner completely (remove from screen)
                if let Some(spinner) = self.current_spinner.take() {
                    spinner.clear();
                }
                
                // Set flag to suppress main section heading (already shown)
                self.suppress_main_heading = true;
                
                // Render content (main heading will be suppressed)
                self.render_data_immediately(data).await?;
                
                // Reset flag
                self.suppress_main_heading = false;
                
                // Move to next section
                self.next_section_index += 1;
                
                // Start next section (title + spinner if enabled)
                self.start_next_section();
            } else {
                // Data not ready for this section yet, stop here
                break;
            }
        }
        
        // Check if we're done with the operation
        if self.next_section_index >= self.expected_sections.len() {
            self.cleanup_operation();
        }
        
        Ok(())
    }
    
    /// Get section key for OutputData type
    fn get_section_key(&self, data: &OutputData) -> Option<&'static str> {
        match data {
            OutputData::StackDefinition(..) => Some("stack_definition"),
            OutputData::StackEvents(..) => Some("stack_events"),
            OutputData::StackContents(..) => Some("stack_contents"),
            OutputData::StackList(..) => Some("stack_list"),
            OutputData::StackDrift(..) => Some("stack_drift"),
            OutputData::ChangeSetResult(..) => Some("changeset_result"),
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
        self.next_section_index = 0;
        self.suppress_main_heading = false;
        self.printed_sections.clear();
    }
    
    /// Render data immediately without ordering logic
    async fn render_data_immediately(&mut self, data: OutputData) -> Result<()> {
        match data {
            OutputData::StackDefinition(ref def, show_times) => self.render_stack_definition(def, show_times).await,
            OutputData::StackEvents(ref events) => self.render_stack_events(events).await,
            OutputData::StackContents(ref contents) => self.render_stack_contents(contents).await,
            OutputData::StatusUpdate(ref update) => self.render_status_update(update).await,
            OutputData::CommandResult(ref result) => self.render_command_result(result).await,
            OutputData::StackList(ref list) => self.render_stack_list(list).await,
            OutputData::ChangeSetResult(ref result) => self.render_changeset_result(result).await,
            OutputData::StackDrift(ref drift) => self.render_stack_drift(drift).await,
            OutputData::Error(ref error) => self.render_error(error).await,
            OutputData::TokenInfo(ref token) => self.render_token_info(token).await,
            _ => Ok(()), // CommandMetadata handled elsewhere
        }
    }
    
}

impl InteractiveRenderer {
    /// Render command metadata (exact iidy-js showCommandSummary implementation)
    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()> {
        self.print_section_heading_with_newline("Command Metadata");
        
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
        
        // Primary Token (following iidy-js pattern)
        self.print_section_entry("Primary Token:", &format!("{} ({})", 
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
        
        println!();
        
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
            println!(" {}", "No events found".color(self.theme.muted));
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
    
    /// Render stack list (exact iidy-js listStacks implementation)
    async fn render_stack_list(&mut self, data: &StackListDisplay) -> Result<()> {
        if data.stacks.is_empty() {
            println!("No stacks found");
            return Ok(());
        }
        
        // Header
        let header = if data.show_tags {
            "Creation/Update Time, Status, Name, Tags"
        } else {
            "Creation/Update Time, Status, Name"
        };
        println!("{}", header.color(self.theme.muted));
        
        // Calculate padding for status column
        let status_padding = self.calc_padding(&data.stacks, |s| &s.stack_status);
        
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
            
            // Format timestamp in iidy-js format for list-stacks
            let timestamp = if let Some(time) = &stack.last_updated_time {
                self.render_event_timestamp(time) // Use event timestamp format
            } else if let Some(time) = &stack.creation_time {
                self.render_event_timestamp(time) // Use event timestamp format
            } else {
                "Unknown".to_string()
            };
            
            let tags_display = if data.show_tags {
                format!(" {}", self.pretty_format_tags(&stack.tags).color(self.theme.muted))
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
    
}
