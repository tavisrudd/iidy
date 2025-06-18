//! CLI Context - Complete CLI argument context for command handlers
//!
//! This module provides a comprehensive CLI context structure that matches
//! the iidy-js GenericCLIArguments pattern, allowing command handlers to
//! access the full CLI state rather than cherry-picking individual fields.

use crate::cli::{GlobalOpts, AwsOpts, NormalizedAwsOpts};

/// Complete CLI context passed to all command handlers
/// 
/// This structure provides access to all CLI arguments and options,
/// matching the iidy-js GenericCLIArguments pattern where the full
/// argv object is passed through the system.
#[derive(Debug, Clone)]
pub struct CliContext {
    /// Global options (environment, debug, theme, etc.)
    pub global_opts: GlobalOpts,
    
    /// Raw AWS options from CLI
    pub aws_opts: AwsOpts,
    
    /// Normalized AWS options with guaranteed token
    pub normalized_aws_opts: NormalizedAwsOpts,
    
    /// Command being executed (for $envValues and CommandsBefore)
    pub command: Vec<String>,
    
    /// Stack name from CLI (if provided)
    pub stack_name: Option<String>,
    
    /// Arguments file path
    pub argsfile: String,
}

impl CliContext {
    /// Create CLI context from command-line arguments
    pub fn new(
        global_opts: GlobalOpts,
        aws_opts: AwsOpts,
        command_parts: &[&str],
        stack_name: Option<String>,
        argsfile: String,
    ) -> Self {
        let normalized_aws_opts = aws_opts.clone().normalize();
        
        Self {
            global_opts,
            aws_opts,
            normalized_aws_opts,
            command: command_parts.iter().map(|s| s.to_string()).collect(),
            stack_name,
            argsfile,
        }
    }
    
    /// Get environment string
    pub fn environment(&self) -> &str {
        &self.global_opts.environment
    }
    
    /// Get command string (equivalent to iidy-js argv._.join(' '))
    pub fn command_string(&self) -> String {
        self.command.join(" ")
    }
    
    /// Get client request token
    pub fn client_request_token(&self) -> &str {
        &self.normalized_aws_opts.client_request_token.value
    }
}