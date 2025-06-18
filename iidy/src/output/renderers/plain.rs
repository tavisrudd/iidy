//! Plain text renderer for CI/non-interactive environments
//!
//! This renderer strips all colors, spinners, and dynamic elements,
//! providing clean, linear output suitable for CI logs and non-TTY environments.

use crate::output::data::*;
use crate::output::renderer::OutputRenderer;
use async_trait::async_trait;
use anyhow::Result;

/// Plain text renderer - no colors, no spinners, CI-friendly
pub struct PlainTextRenderer {
    /// Configuration options
    options: PlainTextOptions,
}

#[derive(Debug, Clone)]
pub struct PlainTextOptions {
    pub show_timestamps: bool,
    pub max_line_width: Option<usize>,
}

impl Default for PlainTextOptions {
    fn default() -> Self {
        Self {
            show_timestamps: true,
            max_line_width: Some(120),
        }
    }
}

impl PlainTextRenderer {
    pub fn new(options: PlainTextOptions) -> Self {
        Self { options }
    }
}

#[async_trait]
impl OutputRenderer for PlainTextRenderer {
    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()> {
        println!();
        println!("Command Metadata:");
        println!(" CFN Operation:        {}", data.cfn_operation);
        println!(" iidy Environment:     {}", data.iidy_environment);
        println!(" Region:               {}", data.region);
        
        if let Some(profile) = &data.profile {
            println!(" Profile:              {}", profile);
        }
        
        // Format CLI arguments
        let cli_args: Vec<String> = data.cli_arguments.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        println!(" CLI Arguments:        {}", cli_args.join(", "));
        
        if let Some(service_role) = &data.iam_service_role {
            println!(" IAM Service Role:     {}", service_role);
        } else {
            println!(" IAM Service Role:     None");
        }
        
        println!(" Current IAM Principal: {}", data.current_iam_principal);
        println!(" iidy Version:         {}", data.iidy_version);
        
        // Token information
        println!(" Primary Token:        {} ({})", 
            data.primary_token.value, 
            format_token_source(&data.primary_token.source)
        );
        
        if !data.derived_tokens.is_empty() {
            println!(" Derived Tokens:       {} tokens", data.derived_tokens.len());
            for (i, token) in data.derived_tokens.iter().enumerate() {
                println!("   [{}] {} ({})", 
                    i + 1, 
                    token.value, 
                    format_token_source(&token.source)
                );
            }
        }
        
        println!();
        Ok(())
    }

    async fn render_stack_definition(&mut self, data: &StackDefinition, show_times: bool) -> Result<()> {
        println!("Stack Details:");
        
        // Handle StackSet name display
        if let Some(stackset_name) = &data.stackset_name {
            println!(" Name (StackSet):      {} {}", data.name, stackset_name);
        } else {
            println!(" Name:                 {}", data.name);
        }
        
        if let Some(description) = &data.description {
            println!(" Description:          {}", description);
        }
        
        println!(" Status:               {}", data.status);
        
        // Capabilities
        if data.capabilities.is_empty() {
            println!(" Capabilities:         None");
        } else {
            println!(" Capabilities:         {}", data.capabilities.join(", "));
        }
        
        // Service Role
        if let Some(service_role) = &data.service_role {
            println!(" Service Role:         {}", service_role);
        } else {
            println!(" Service Role:         None");
        }
        
        // Tags
        if data.tags.is_empty() {
            println!(" Tags:                 None");
        } else {
            let tags: Vec<String> = data.tags.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            println!(" Tags:                 {}", tags.join(", "));
        }
        
        // Parameters
        if data.parameters.is_empty() {
            println!(" Parameters:           None");
        } else {
            let params: Vec<String> = data.parameters.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            println!(" Parameters:           {}", params.join(", "));
        }
        
        println!(" DisableRollback:      {}", data.disable_rollback);
        
        let protection_text = if data.termination_protection {
            format!("{} (protected)", data.termination_protection)
        } else {
            data.termination_protection.to_string()
        };
        println!(" TerminationProtection: {}", protection_text);
        
        // Times (conditional)
        if show_times {
            if let Some(creation_time) = &data.creation_time {
                println!(" Creation Time:        {}", creation_time.format("%Y-%m-%d %H:%M:%S UTC"));
            }
            if let Some(last_updated_time) = &data.last_updated_time {
                println!(" Last Update Time:     {}", last_updated_time.format("%Y-%m-%d %H:%M:%S UTC"));
            }
        }
        
        // Timeout
        if let Some(timeout) = data.timeout_in_minutes {
            println!(" Timeout In Minutes:   {}", timeout);
        }
        
        // NotificationARNs
        if data.notification_arns.is_empty() {
            println!(" NotificationARNs:     None");
        } else {
            println!(" NotificationARNs:     {}", data.notification_arns.join(", "));
        }
        
        // Stack Policy
        if let Some(policy) = &data.stack_policy {
            println!(" Stack Policy Source:  {}", policy);
        }
        
        // ARN and Console URL
        println!(" ARN:                  {}", data.arn);
        println!(" Console URL:          {}", data.console_url);
        
        println!();
        Ok(())
    }

    async fn render_stack_events(&mut self, data: &StackEventsDisplay) -> Result<()> {
        println!("{}", data.title);
        
        if data.events.is_empty() {
            println!(" No events to display");
            return Ok(());
        }
        
        // Calculate column widths for alignment
        let time_width = 20; // "YYYY-MM-DD HH:MM:SS"
        let status_width = data.events.iter()
            .map(|e| e.event.resource_status.len())
            .max()
            .unwrap_or(15)
            .max(15);
        let resource_id_width = data.events.iter()
            .map(|e| e.event.logical_resource_id.len())
            .max()
            .unwrap_or(20)
            .max(20);
        
        // Header
        println!(" {:<time_width$} {:<status_width$} {:<resource_id_width$} Type / Reason", 
            "Timestamp", "Status", "ResourceId",
            time_width = time_width,
            status_width = status_width,
            resource_id_width = resource_id_width
        );
        println!(" {}", "-".repeat(time_width + status_width + resource_id_width + 20));
        
        for event_with_timing in &data.events {
            let event = &event_with_timing.event;
            
            // Format timestamp
            let timestamp_str = if let Some(timestamp) = &event.timestamp {
                timestamp.format("%Y-%m-%d %H:%M:%S").to_string()
            } else {
                "N/A".to_string()
            };
            
            // Main event line
            println!(" {:<time_width$} {:<status_width$} {:<resource_id_width$} {}", 
                timestamp_str,
                event.resource_status,
                event.logical_resource_id,
                event.resource_type,
                time_width = time_width,
                status_width = status_width,
                resource_id_width = resource_id_width
            );
            
            // Show duration if available
            if let Some(duration) = event_with_timing.duration_seconds {
                println!("   Duration: {}s", duration);
            }
            
            // Show reason if available
            if let Some(reason) = &event.resource_status_reason {
                if !reason.is_empty() {
                    println!("   Reason: {}", reason);
                }
            }
            
            // Show physical resource ID if different and available
            if let Some(physical_id) = &event.physical_resource_id {
                if physical_id != &event.logical_resource_id && !physical_id.is_empty() {
                    println!("   Physical ID: {}", physical_id);
                }
            }
        }
        
        // Show truncation message
        if let Some(truncation) = &data.truncated {
            println!(" {} of {} total events shown", truncation.shown, truncation.total);
        }
        
        println!();
        Ok(())
    }

    async fn render_stack_contents(&mut self, data: &StackContents) -> Result<()> {
        // Stack Resources
        if !data.resources.is_empty() {
            println!("Stack Resources:");
            
            let id_width = data.resources.iter()
                .map(|r| r.logical_resource_id.len())
                .max()
                .unwrap_or(20)
                .max(20);
            let type_width = data.resources.iter()
                .map(|r| r.resource_type.len())
                .max()
                .unwrap_or(30)
                .max(30);
            
            for resource in &data.resources {
                println!(" {:<id_width$} {:<type_width$} {}", 
                    resource.logical_resource_id,
                    resource.resource_type,
                    resource.physical_resource_id.as_deref().unwrap_or(""),
                    id_width = id_width,
                    type_width = type_width
                );
            }
            println!();
        }
        
        // Stack Outputs
        println!("Stack Outputs:");
        if data.outputs.is_empty() {
            println!(" None");
        } else {
            let key_width = data.outputs.iter()
                .map(|o| o.output_key.len())
                .max()
                .unwrap_or(20)
                .max(20);
            
            for output in &data.outputs {
                println!(" {:<width$} {}", 
                    output.output_key, 
                    output.output_value,
                    width = key_width
                );
            }
        }
        println!();
        
        // Stack Exports
        if !data.exports.is_empty() {
            println!("Stack Exports:");
            let name_width = data.exports.iter()
                .map(|e| e.name.len())
                .max()
                .unwrap_or(20)
                .max(20);
            
            for export in &data.exports {
                println!(" {:<width$} {}", 
                    export.name, 
                    export.value,
                    width = name_width
                );
                
                // Show imports
                for import in &export.importing_stacks {
                    println!("   imported by {}", import);
                }
            }
            println!();
        }
        
        // Current Stack Status
        println!("Current Stack Status: {} {}", 
            data.current_status.status,
            data.current_status.status_reason.as_deref().unwrap_or("")
        );
        
        // Pending Changesets
        if !data.pending_changesets.is_empty() {
            println!();
            println!("Pending Changesets:");
            for changeset in &data.pending_changesets {
                if let Some(creation_time) = &changeset.creation_time {
                    println!(" {} {} {} {}", 
                        creation_time.format("%Y-%m-%d %H:%M:%S"),
                        changeset.change_set_name,
                        changeset.status,
                        changeset.status_reason.as_deref().unwrap_or("")
                    );
                } else {
                    println!(" {} {} {}", 
                        changeset.change_set_name,
                        changeset.status,
                        changeset.status_reason.as_deref().unwrap_or("")
                    );
                }
                
                if let Some(description) = &changeset.description {
                    if !description.is_empty() {
                        println!("   Description: {}", description);
                    }
                }
            }
        }
        
        println!();
        Ok(())
    }

    async fn render_status_update(&mut self, data: &StatusUpdate) -> Result<()> {
        let level_prefix = match data.level {
            StatusLevel::Info => "[INFO]",
            StatusLevel::Warning => "[WARN]",
            StatusLevel::Error => "[ERROR]",
            StatusLevel::Success => "[SUCCESS]",
        };
        
        if self.options.show_timestamps {
            println!("{} {} {}", 
                data.timestamp.format("%H:%M:%S"),
                level_prefix,
                data.message
            );
        } else {
            println!("{} {}", level_prefix, data.message);
        }
        
        Ok(())
    }

    async fn render_command_result(&mut self, data: &CommandResult) -> Result<()> {
        let status = if data.success { "SUCCESS" } else { "FAILED" };
        println!();
        println!("Command Result: {} ({}s)", status, data.elapsed_seconds);
        
        if let Some(message) = &data.message {
            println!("Message: {}", message);
        }
        
        println!("Exit Code: {}", data.exit_code);
        println!();
        Ok(())
    }

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
        println!("{}", header);
        
        // Calculate column widths
        let time_width = 20;
        let status_width = data.stacks.iter()
            .map(|s| s.stack_status.len())
            .max()
            .unwrap_or(15)
            .max(15);
        
        for stack in &data.stacks {
            // Format creation/update time
            let time_str = stack.last_updated_time
                .or(stack.creation_time)
                .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "N/A".to_string());
            
            // Lifecycle indicators
            let lifecycle_icon = if stack.termination_protection {
                "[PROTECTED] "
            } else {
                match stack.environment_type.as_deref() {
                    Some("production") => "[PROD] ",
                    Some("integration") => "[INTEG] ",
                    Some("development") => "[DEV] ",
                    _ => "",
                }
            };
            
            // Main line
            print!("{:<time_width$} {:<status_width$} {}{}", 
                time_str,
                stack.stack_status,
                lifecycle_icon,
                stack.stack_name,
                time_width = time_width,
                status_width = status_width
            );
            
            // Tags
            if data.show_tags && !stack.tags.is_empty() {
                let tags: Vec<String> = stack.tags.iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect();
                print!(" {}", tags.join(","));
            }
            
            println!();
            
            // Failure reason on next line
            if stack.stack_status.contains("FAILED") {
                if let Some(reason) = &stack.status_reason {
                    if !reason.is_empty() {
                        println!("   {}", reason);
                    }
                }
            }
        }
        
        println!();
        Ok(())
    }

    async fn render_changeset_result(&mut self, data: &ChangeSetCreationResult) -> Result<()> {
        println!();
        println!("Changeset Creation Result:");
        println!(" Changeset Name:       {}", data.changeset_name);
        println!(" Stack Name:           {}", data.stack_name);
        println!(" Changeset Type:       {}", data.changeset_type);
        println!(" Status:               {}", data.status);
        println!(" Has Changes:          {}", data.has_changes);
        println!(" Console URL:          {}", data.console_url);
        
        if !data.next_steps.is_empty() {
            println!();
            println!("Next Steps:");
            for (i, step) in data.next_steps.iter().enumerate() {
                println!(" {}. {}", i + 1, step);
            }
        }
        
        if !data.pending_changesets.is_empty() {
            println!();
            println!("Pending Changesets:");
            for changeset in &data.pending_changesets {
                println!(" {} ({})", changeset.change_set_name, changeset.status);
            }
        }
        
        println!();
        Ok(())
    }

    async fn render_stack_drift(&mut self, data: &StackDrift) -> Result<()> {
        println!();
        if data.drifted_resources.is_empty() {
            println!("No drift detected. Stack resources are in sync with template.");
        } else {
            println!("Drifted Resources:");
            for drift in &data.drifted_resources {
                println!("{} {} {}", drift.logical_resource_id, drift.resource_type, drift.physical_resource_id);
                println!("  {}", drift.drift_status);
                
                if !drift.property_differences.is_empty() {
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
        Ok(())
    }

    async fn render_error(&mut self, data: &ErrorInfo) -> Result<()> {
        println!();
        println!("ERROR [{}]: {}", data.error_type, data.message);
        
        if let Some(details) = &data.details {
            println!("Details: {}", details);
        }
        
        if !data.suggestions.is_empty() {
            println!();
            println!("Suggestions:");
            for suggestion in &data.suggestions {
                println!(" - {}", suggestion);
            }
        }
        
        println!();
        Ok(())
    }

    async fn init(&mut self) -> Result<()> {
        // Plain text renderer needs no initialization
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        // Plain text renderer needs no cleanup
        Ok(())
    }
}

/// Helper function to format token source for display
fn format_token_source(source: &TokenSource) -> String {
    match source {
        TokenSource::UserProvided => "user-provided".to_string(),
        TokenSource::AutoGenerated => "auto-generated".to_string(),
        TokenSource::Derived { from, step } => format!("derived from {} at {}", from, step),
    }
}