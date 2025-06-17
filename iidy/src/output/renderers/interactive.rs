//! Interactive renderer with exact iidy-js formatting
//!
//! This renderer provides pixel-perfect output matching the original iidy-js implementation,
//! including colors, spacing, timestamps, and all formatting details.

use crate::output::data::*;
use crate::output::renderer::OutputRenderer;
use crate::output::theme::{IidyTheme, get_terminal_width};
use crate::cli::{Theme, ColorChoice};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::io::{self, Write};

// Core constants matching iidy-js exactly (from complete implementation spec)
pub const COLUMN2_START: usize = 25;
// Removed DEFAULT_STATUS_PADDING as it's not used in this exact implementation
pub const MIN_STATUS_PADDING: usize = 17;
pub const MAX_PADDING: usize = 60;
pub const RESOURCE_TYPE_PADDING: usize = 40;

/// Configuration options for interactive rendering
#[derive(Debug, Clone, Copy)]
pub struct InteractiveOptions {
    pub theme: Theme,
    pub color_choice: ColorChoice,
    pub terminal_width: Option<usize>,
    pub show_timestamps: bool,
}

impl Default for InteractiveOptions {
    fn default() -> Self {
        Self {
            theme: Theme::Auto,
            color_choice: ColorChoice::Auto,
            terminal_width: None, // Will auto-detect
            show_timestamps: true,
        }
    }
}

/// Interactive renderer with exact iidy-js formatting and colors
pub struct InteractiveRenderer {
    options: InteractiveOptions,
    theme: IidyTheme,
    #[allow(dead_code)] // Will be used for text wrapping in future
    terminal_width: usize,
}

impl InteractiveRenderer {
    pub fn new(options: InteractiveOptions) -> Self {
        let theme = IidyTheme::new(options.theme, options.color_choice);
        let terminal_width = options.terminal_width.unwrap_or_else(get_terminal_width);
        
        Self { 
            options,
            theme,
            terminal_width,
        }
    }
    
    /// Check if colors are enabled
    fn colors_enabled(&self) -> bool {
        self.theme.colors_enabled
    }
    
    /// Format section heading (exact iidy-js implementation)
    fn format_section_heading(&self, text: &str) -> String {
        if self.colors_enabled() {
            format!("{}:", text.white())
        } else {
            format!("{}:", text)
        }
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
    
    /// Render timestamp in iidy-js format
    fn render_timestamp(&self, dt: &DateTime<Utc>) -> String {
        dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
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
}

#[async_trait]
impl OutputRenderer for InteractiveRenderer {
    async fn init(&mut self) -> Result<()> {
        // Interactive renderer doesn't need initialization
        Ok(())
    }
    
    async fn cleanup(&mut self) -> Result<()> {
        // Flush any remaining output
        io::stdout().flush()?;
        Ok(())
    }
    
    /// Render command metadata (exact iidy-js showCommandSummary implementation)
    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()> {
        println!(); // blank line
        println!("{}", self.format_section_heading("Command Metadata"));
        
        self.print_section_entry("CFN Operation:", &data.cfn_operation.color(self.theme.primary).to_string())?;
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
        
        // Derived Tokens (following iidy-js pattern)
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
        println!("{}", self.format_section_heading("Stack Details"));
        
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
        println!("{}", self.format_section_heading(&data.title));
        
        if data.events.is_empty() {
            println!(" {}", "No events found".color(self.theme.muted));
            return Ok(());
        }
        
        // Calculate padding
        let logical_id_padding = self.calc_padding(&data.events, |e| &e.event.logical_resource_id);
        let resource_type_padding = std::cmp::min(RESOURCE_TYPE_PADDING, MAX_PADDING);
        
        for event_with_timing in &data.events {
            let event = &event_with_timing.event;
            
            // Format timestamp
            let timestamp = if let Some(ts) = &event.timestamp {
                self.format_timestamp(&self.render_timestamp(ts))
            } else {
                self.format_timestamp("                        ")
            };
            
            // Format duration if available
            let duration_text = if let Some(duration) = event_with_timing.duration_seconds {
                format!(" ({}s)", duration)
            } else {
                String::new()
            };
            
            // Format logical ID with padding
            let logical_id = self.format_logical_id(&format!(" {:<width$}", 
                event.logical_resource_id, 
                width = logical_id_padding
            ));
            
            // Format resource type with padding
            let resource_type = format!("{:<width$}", 
                event.resource_type, 
                width = resource_type_padding
            );
            
            // Format status
            let status = self.colorize_resource_status(&event.resource_status, None);
            
            // Status reason
            let status_reason = event.resource_status_reason.as_deref().unwrap_or("");
            
            println!("{} {} {} {} {}{}",
                timestamp,
                logical_id,
                resource_type.color(self.theme.muted),
                status,
                status_reason.color(self.theme.muted),
                duration_text.color(self.theme.muted)
            );
        }
        
        // Show truncation info if present
        if let Some(truncation) = &data.truncated {
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
        // Stack Resources
        if !data.resources.is_empty() {
            println!("{}", self.format_section_heading("Stack Resources"));
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
        
        println!();
        
        // Stack Outputs
        print!("{}", self.format_section_heading("Stack Outputs"));
        if data.outputs.is_empty() {
            println!(" {}", "None".color(self.theme.muted));
        } else {
            println!();
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
            println!();
            println!("{}", self.format_section_heading("Stack Exports"));
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
        
        println!();
        
        // Current Stack Status
        println!("{} {} {}",
            self.format_section_heading(&format!("{:<width$}", "Current Stack Status", width = COLUMN2_START)),
            self.colorize_resource_status(&data.current_status.status, None),
            data.current_status.status_reason.as_deref().unwrap_or("").color(self.theme.muted)
        );
        
        // Pending Changesets
        if !data.pending_changesets.is_empty() {
            println!();
            println!("{}", self.format_section_heading("Pending Changesets"));
            
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
                data.message.color(self.theme.warning).to_string() // iidy-js: yellow for warning messages
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
        let status_text = if data.success {
            self.format_section_heading("SUCCESS")
        } else {
            "FAILURE".color(self.theme.error).to_string()
        };
        
        println!();
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
        
        // Calculate padding
        let time_padding = 24;
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
            
            // Main line output
            let timestamp = if let Some(time) = &stack.last_updated_time {
                self.render_timestamp(time)
            } else if let Some(time) = &stack.creation_time {
                self.render_timestamp(time)
            } else {
                "Unknown".to_string()
            };
            
            let tags_display = if data.show_tags {
                format!(" {}", self.pretty_format_tags(&stack.tags).color(self.theme.muted))
            } else {
                String::new()
            };
            
            println!("{} {} {}{}{}",
                self.format_timestamp(&format!("{:>width$}", timestamp, width = time_padding)),
                self.colorize_resource_status(&stack.stack_status, Some(status_padding)),
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
        println!();
        
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
}