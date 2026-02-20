//! Interactive renderer
//!
//! This renderer handles all console output and interactions,
//! including colors, spacing, timestamps, other formatting details,
//! spinners, and confirmations.

use crate::cfn::CfnOperation;
use crate::cli::{ColorChoice, Commands, Theme};
use crate::output::data::*;
use crate::output::renderer::OutputRenderer;
use crate::output::spinner::{Spinner, SpinnerStyle};
use crate::output::theme::{IidyTheme, get_terminal_width};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::io::{self, IsTerminal, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use textwrap;

pub const COLUMN2_START: usize = 25;
pub const MIN_STATUS_PADDING: usize = 17;
pub const MAX_PADDING: usize = 60;
pub const RESOURCE_TYPE_PADDING: usize = 40;

type TimingState = Arc<Mutex<(DateTime<Utc>, Option<DateTime<Utc>>)>>;

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

pub struct InteractiveRenderer {
    options: InteractiveOptions,
    theme: IidyTheme,
    terminal_width: usize, // for text wrapping
    has_rendered_content: bool,
    // Async ordering state
    current_operation: Option<String>,
    expected_sections: Vec<&'static str>,
    pending_sections: std::collections::HashMap<String, OutputData>,
    current_spinner: Option<Spinner>,
    next_section_index: usize,
    suppress_main_heading: bool,
    printed_sections: Vec<String>, // Track which section titles have been printed
    cli_context: Option<Arc<crate::cli::Cli>>,
    // Section titles configured during construction
    section_titles: HashMap<String, String>,
    // Live events timing state (local to spinner)
    timing_task_handle: Option<tokio::task::JoinHandle<()>>,
    timing_state: Option<TimingState>, // (start_time, last_live_event_time)
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

    fn are_colors_enabled(&self) -> bool {
        self.theme.colors_enabled
    }

    fn format_section_heading(&self, text: &str) -> String {
        // Remove trailing colon if present to avoid double colons
        let clean_text = text.trim_end_matches(':');

        if self.are_colors_enabled() {
            format!("{}:", clean_text.color(self.theme.section_heading).bold())
        } else {
            format!("{clean_text}:")
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

    fn add_content_spacing(&mut self) {
        if self.has_rendered_content {
            println!();
        }
        self.has_rendered_content = true;
    }

    /// Format section label
    fn format_section_label(&self, text: &str) -> String {
        if self.are_colors_enabled() {
            text.color(self.theme.muted).to_string() // iidy-js: truecolor(128, 128, 128) - blackBright for section labels
        } else {
            text.to_string()
        }
    }

    /// Format section entry
    fn format_section_entry(&self, label: &str, data: &str) -> String {
        format!(
            " {}{}\n",
            self.format_section_label(&format!("{:<width$} ", label, width = COLUMN2_START - 1)),
            data
        )
    }

    /// Print section entry to stdout
    fn print_section_entry(&self, label: &str, data: &str) -> Result<()> {
        print!("{}", self.format_section_entry(label, data));
        io::stdout().flush()?;
        Ok(())
    }

    fn format_logical_id(&self, text: &str) -> String {
        if self.are_colors_enabled() {
            text.color(self.theme.resource_id).to_string() // iidy-js: xterm color 252 - light gray for logical resource IDs
        } else {
            text.to_string()
        }
    }

    fn format_timestamp(&self, text: &str) -> String {
        if self.are_colors_enabled() {
            text.color(self.theme.timestamp).to_string() // iidy-js: xterm color 253 - light gray for timestamps
        } else {
            text.to_string()
        }
    }

    /// Render timestamp in canonical format for all timestamps
    fn render_timestamp(&self, dt: &DateTime<Utc>) -> String {
        dt.format("%a %b %d %Y %H:%M:%S").to_string()
    }

    fn render_event_timestamp(&self, dt: &DateTime<Utc>) -> String {
        self.render_timestamp(dt)
    }

    fn colorize_resource_status(&self, status: &str, padding: Option<usize>) -> String {
        if !self.are_colors_enabled() {
            return match padding {
                Some(width) => format!("{status:<width$}"),
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
            Some(width) => format!("{colored_status:<width$}"),
            None => colored_status,
        }
    }

    /// Calculate padding for a collection of items
    fn calc_padding<T, F>(&self, items: &[T], extractor: F) -> usize
    where
        F: Fn(&T) -> &str,
    {
        let max_len = items
            .iter()
            .map(|item| extractor(item).len())
            .max()
            .unwrap_or(0);

        max_len.clamp(MIN_STATUS_PADDING, MAX_PADDING)
    }

    fn pretty_format_tags(&self, tags: &HashMap<String, String>) -> String {
        self.pretty_format_tags_with_truncation(tags, None)
    }

    /// Pretty format tags with optional truncation and Environment tag prioritization
    fn pretty_format_tags_with_truncation(
        &self,
        tags: &HashMap<String, String>,
        max_tags: Option<usize>,
    ) -> String {
        if tags.is_empty() {
            return String::new();
        }

        let mut formatted_tags = Vec::new();

        // First, add Environment/environment tag if it exists (case-insensitive)
        let env_keys = ["Environment", "environment", "ENVIRONMENT", "env", "ENV"];
        for env_key in &env_keys {
            if let Some(env_value) = tags.get(*env_key) {
                formatted_tags.push(format!("{env_key}={env_value}"));
                break; // Only add the first match
            }
        }

        // Then add other tags (excluding the environment tag we already added)
        let mut other_tags: Vec<String> = tags
            .iter()
            .filter(|(key, _)| !env_keys.contains(&key.as_str()))
            .map(|(key, value)| format!("{key}={value}"))
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

    fn pretty_format_parameters(&self, params: &HashMap<String, String>) -> String {
        if params.is_empty() {
            return String::new();
        }

        let mut formatted_params: Vec<String> = params
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect();
        formatted_params.sort();
        formatted_params.join(", ")
    }

    fn pretty_format_small_map(&self, map: &HashMap<String, String>) -> String {
        if map.is_empty() {
            return String::new();
        }

        let mut items: Vec<String> = map.iter().map(|(k, v)| format!("{k}={v}")).collect();
        items.sort();
        items.join(", ")
    }

    fn color_by_environment(&self, text: &str, env_name: &str) -> String {
        if !self.are_colors_enabled() {
            return text.to_string();
        }

        match env_name {
            "production" => text.color(self.theme.env_production).to_string(), // iidy-js: red for production environments
            "integration" => text.color(self.theme.env_integration).to_string(), // iidy-js: xterm color 75 - blue-ish for integration
            "development" => text.color(self.theme.env_development).to_string(), // iidy-js: xterm color 194 - yellow-ish for development
            _ => text.to_string(),
        }
    }

    fn format_token_source(&self, source: &TokenSource) -> String {
        match source {
            TokenSource::UserProvided => "user-provided".to_string(),
            TokenSource::AutoGenerated => "auto-generated".to_string(),
            TokenSource::Derived { from, step } => format!("derived from {from} at {step}"),
        }
    }

    /// Create spinner for API waiting periods if in TTY and enabled
    fn create_api_spinner(&self, message: &str) -> Option<Spinner> {
        if self.options.enable_spinners && self.are_colors_enabled() && io::stdout().is_terminal() {
            let styled_message = self.style_muted_text(message);
            Some(Spinner::with_style(SpinnerStyle::Dots12, &styled_message))
        } else {
            // Spinners disabled, non-TTY, or colors disabled: just print the message
            println!("{message}");
            None
        }
    }

    fn format_timing_text(
        total_elapsed: i64,
        last_live_event: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> String {
        if let Some(last_event_time) = last_live_event {
            let since_last_event = (now - last_event_time).num_seconds();
            format!("{total_elapsed} seconds elapsed total. {since_last_event} since last event.")
        } else {
            format!("{total_elapsed} seconds elapsed total.")
        }
    }

    fn style_muted_text(&self, text: &str) -> String {
        text.color(self.theme.muted).to_string()
    }

    fn start_live_events_timing(&mut self, start_time: DateTime<Utc>) {
        if let Some(spinner) = &self.current_spinner
            && let Some(spinner_ref) = spinner.get_spinner_ref()
        {
            // Create shared state for timing info (start_time, last_live_event_time)
            // Start with None for last_live_event_time since we haven't seen any live events yet
            let timing_state = Arc::new(Mutex::new((start_time, None)));
            let timing_ref = Arc::clone(&timing_state);

            let muted_color = self.theme.muted;

            // Spawn background task that updates spinner every second
            let task = tokio::spawn(async move {
                let mut interval =
                    tokio::time::interval(Duration::from_secs(LIVE_EVENTS_UPDATE_INTERVAL_SECS));

                loop {
                    interval.tick().await;

                    let timing_guard = timing_ref.lock().unwrap();
                    let (start, last_live_event): (DateTime<Utc>, Option<DateTime<Utc>>) =
                        *timing_guard;
                    drop(timing_guard);

                    let now = Utc::now();
                    let total_elapsed = (now - start).num_seconds();
                    let timing_text = Self::format_timing_text(total_elapsed, last_live_event, now);
                    let styled_text = timing_text.color(muted_color).to_string();
                    spinner_ref.set_message(styled_text);
                }
            });

            self.timing_task_handle = Some(task);
            self.timing_state = Some(timing_state);
        }
    }

    fn update_last_event_time(&mut self, event_time: DateTime<Utc>) {
        if let Some(timing_state) = &self.timing_state
            && let Ok(mut state) = timing_state.lock()
        {
            state.1 = Some(event_time); // Update last_live_event_time
        }
    }

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
    async fn render_output_data(
        &mut self,
        data: OutputData,
        buffer: Option<&VecDeque<OutputData>>,
    ) -> Result<()> {
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
            CfnOperation::DescribeStack => {
                vec!["stack_definition", "stack_events", "stack_contents"]
            }
            CfnOperation::ListStacks => vec!["stack_list"],
            CfnOperation::WatchStack => vec![
                "stack_definition",
                "stack_events",
                "live_stack_events",
                "stack_contents",
            ],
            CfnOperation::GetStackTemplate => vec![], // Exception: doesn't use renderer system
            CfnOperation::DescribeStackDrift => vec!["stack_drift"],

            // Modification operations with monitoring (include command_metadata as first section)
            CfnOperation::CreateStack => vec![
                "command_metadata",
                "stack_definition",
                "live_stack_events",
                "stack_contents",
            ],
            CfnOperation::DeleteStack => {
                // Check if --yes flag is set to skip confirmation section
                if let Commands::DeleteStack(args) = &cli.command {
                    if args.yes {
                        // Skip confirmation section when --yes is used
                        vec![
                            "command_metadata",
                            "stack_definition",
                            "stack_events",
                            "stack_contents",
                            "live_stack_events",
                        ]
                    } else {
                        // Include confirmation section when --yes is not used
                        vec![
                            "command_metadata",
                            "stack_definition",
                            "stack_events",
                            "stack_contents",
                            "confirmation",
                            "live_stack_events",
                        ]
                    }
                } else {
                    // Fallback (shouldn't happen)
                    vec![
                        "command_metadata",
                        "stack_definition",
                        "stack_events",
                        "stack_contents",
                        "confirmation",
                        "live_stack_events",
                    ]
                }
            }
            CfnOperation::UpdateStack => {
                // Check if --changeset flag is set
                if let Commands::UpdateStack(args) = &cli.command {
                    if args.changeset {
                        // Phase 1: Changeset creation
                        if args.yes {
                            // Skip confirmation section when --yes is used
                            vec!["command_metadata", "stack_definition", "changeset_result"]
                        } else {
                            // Include confirmation section when --yes is not used
                            vec![
                                "command_metadata",
                                "stack_definition",
                                "changeset_result",
                                "confirmation",
                            ]
                        }
                    } else {
                        // Regular update-stack flow (no confirmation needed)
                        vec![
                            "command_metadata",
                            "stack_definition",
                            "live_stack_events",
                            "stack_contents",
                        ]
                    }
                } else {
                    // Fallback (shouldn't happen)
                    vec![
                        "command_metadata",
                        "stack_definition",
                        "live_stack_events",
                        "stack_contents",
                    ]
                }
            }
            CfnOperation::CreateChangeset => vec!["command_metadata", "changeset_result"],
            CfnOperation::ExecuteChangeset => vec![
                "command_metadata",
                "stack_definition",
                "stack_events",
                "live_stack_events",
                "stack_contents",
            ],
            CfnOperation::CreateOrUpdate => {
                // Check if --changeset flag is set
                if let Commands::CreateOrUpdate(args) = &cli.command {
                    if args.changeset {
                        // Phase 1: Changeset creation
                        if args.yes {
                            // Skip confirmation section when --yes is used
                            vec!["command_metadata", "stack_definition", "changeset_result"]
                        } else {
                            // Include confirmation section when --yes is not used
                            vec![
                                "command_metadata",
                                "stack_definition",
                                "changeset_result",
                                "confirmation",
                            ]
                        }
                    } else {
                        // Regular create-or-update flow (no confirmation needed)
                        vec![
                            "command_metadata",
                            "stack_change_details",
                            "stack_definition",
                            "live_stack_events",
                            "stack_contents",
                        ]
                    }
                } else {
                    // Fallback (shouldn't happen)
                    vec![
                        "command_metadata",
                        "stack_change_details",
                        "stack_definition",
                        "live_stack_events",
                        "stack_contents",
                    ]
                }
            }

            // Other operations
            CfnOperation::EstimateCost => vec!["command_metadata", "cost_estimate"],
            CfnOperation::TemplateApprovalRequest => vec![
                "command_metadata",
                "template_validation",
                "approval_request_result",
            ],
            CfnOperation::TemplateApprovalReview => vec![
                "command_metadata",
                "approval_status",
                "template_diff",
                "confirmation",
                "approval_result",
            ],
            CfnOperation::ConvertStackToIidy => vec![],
            CfnOperation::LintTemplate => vec!["template_validation"],
        };

        // Start the first section for operations with predefined sections (show title immediately, spinner if enabled)
        // Exception: GetStackTemplate doesn't use the renderer system
        if !matches!(*operation, CfnOperation::GetStackTemplate) {
            self.start_next_section();
        }
    }

    /// Start the next section (show title immediately, spinner if enabled)
    fn start_next_section(&mut self) {
        if self.next_section_index < self.expected_sections.len() {
            let section_key = self.expected_sections[self.next_section_index];
            let title = self.get_section_title(section_key).to_string();

            if title.is_empty() {
                // For confirmation sections: just add blank line separation if not first section
                if self.next_section_index > 0 {
                    println!(); // Blank line before confirmation
                }
            } else {
                // Show section heading immediately (with newline if section is always multi-line)
                if self.section_is_always_multiline(section_key) {
                    self.print_section_heading_with_newline(&title);
                } else {
                    self.print_section_heading(&title);
                }
            }
            if self.options.enable_spinners && !title.is_empty() {
                // Always put spinner on the line after the heading
                if !self.section_is_always_multiline(section_key) {
                    println!(); // Add newline after inline heading for spinner
                }
                self.current_spinner =
                    self.create_api_spinner(&format!("Loading {}...", title.to_lowercase()));

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
        self.section_titles
            .insert("stack_definition".to_string(), "Stack Details".to_string());
        self.section_titles
            .insert("stack_list".to_string(), "Stack List".to_string());
        self.section_titles
            .insert("stack_drift".to_string(), "Stack Drift".to_string());
        self.section_titles.insert(
            "changeset_result".to_string(),
            "Changeset Result".to_string(),
        );
        self.section_titles
            .insert("stack_contents".to_string(), "Stack Resources".to_string());
        self.section_titles
            .insert("stack_events".to_string(), "Stack Events".to_string());
        self.section_titles.insert(
            "command_metadata".to_string(),
            "Command Metadata".to_string(),
        );
        self.section_titles.insert(
            "live_stack_events".to_string(),
            "Live Stack Events".to_string(),
        );
        self.section_titles
            .insert("cost_estimate".to_string(), "Cost Estimate".to_string());
        self.section_titles.insert(
            "template_validation".to_string(),
            "Template Validation".to_string(),
        );

        match operation {
            CfnOperation::DescribeStack => {
                if let Commands::DescribeStack(args) = &cli.command {
                    self.section_titles.insert(
                        "stack_events".to_string(),
                        format!("Previous Stack Events (max {}):", args.events),
                    );
                }
            }
            CfnOperation::WatchStack => {
                // For watch-stack, we have separate previous and live events sections
                self.section_titles.insert(
                    "stack_events".to_string(),
                    "Previous Stack Events (max 10):".to_string(),
                );
                self.section_titles.insert(
                    "live_stack_events".to_string(),
                    "Live Stack Events (2s poll):".to_string(),
                );
            }
            CfnOperation::CreateStack => {
                // For create-stack, include command metadata and live events
                self.section_titles.insert(
                    "command_metadata".to_string(),
                    "Command Metadata:".to_string(),
                );
                self.section_titles
                    .insert("stack_definition".to_string(), "Stack Details".to_string());
                self.section_titles
                    .insert("stack_contents".to_string(), "Stack Resources".to_string());
                self.section_titles.insert(
                    "live_stack_events".to_string(),
                    "Live Stack Events (2s poll):".to_string(),
                );
            }
            CfnOperation::DeleteStack => {
                // For delete-stack, include command metadata, previous events, and live events
                self.section_titles.insert(
                    "command_metadata".to_string(),
                    "Command Metadata:".to_string(),
                );
                self.section_titles
                    .insert("stack_definition".to_string(), "Stack Details".to_string());
                self.section_titles.insert(
                    "stack_events".to_string(),
                    "Previous Stack Events (max 10):".to_string(),
                );
                self.section_titles
                    .insert("stack_contents".to_string(), "Stack Resources".to_string());
                self.section_titles.insert(
                    "live_stack_events".to_string(),
                    "Live Stack Events (2s poll):".to_string(),
                );
            }
            CfnOperation::ExecuteChangeset => {
                // For exec-changeset, include command metadata, previous events, and live events
                self.section_titles.insert(
                    "command_metadata".to_string(),
                    "Command Metadata:".to_string(),
                );
                self.section_titles
                    .insert("stack_definition".to_string(), "Stack Details".to_string());
                self.section_titles.insert(
                    "stack_events".to_string(),
                    "Previous Stack Events (max 10):".to_string(),
                );
                self.section_titles
                    .insert("stack_contents".to_string(), "Stack Resources".to_string());
                self.section_titles.insert(
                    "live_stack_events".to_string(),
                    "Live Stack Events (2s poll):".to_string(),
                );
            }
            CfnOperation::CreateOrUpdate => {
                // For create-or-update, include change details section
                self.section_titles.insert(
                    "command_metadata".to_string(),
                    "Command Metadata:".to_string(),
                );
                self.section_titles.insert(
                    "stack_change_details".to_string(),
                    "Stack Change Details".to_string(),
                );
                self.section_titles
                    .insert("stack_definition".to_string(), "Stack Details".to_string());
                self.section_titles
                    .insert("stack_contents".to_string(), "Stack Resources".to_string());
                self.section_titles.insert(
                    "live_stack_events".to_string(),
                    "Live Stack Events (2s poll):".to_string(),
                );
            }
            _ => {
                self.section_titles
                    .insert("stack_events".to_string(), "Stack Events:".to_string());
            }
        }
    }

    fn get_section_title(&self, section_key: &str) -> &str {
        // Confirmation sections have no title (empty string)
        if section_key == "confirmation" || section_key.starts_with("confirmation_") {
            return "";
        }

        self.section_titles
            .get(section_key)
            .map(|s| s.as_str())
            .unwrap_or("Loading")
    }

    /// Check if a section should always be multi-line (needs newline after heading)
    fn section_is_always_multiline(&self, section_key: &str) -> bool {
        match section_key {
            "command_metadata" => true,  // Command Metadata is always multi-line
            "stack_definition" => true,  // Stack Details is always multi-line
            "stack_contents" => true,    // Stack Resources is always multi-line
            "stack_events" => true,      // Previous Stack Events is always multi-line
            "live_stack_events" => true, // Live Stack Events is always multi-line
            _ => false,
        }
    }

    /// Render data with ordering logic for parallel operations
    async fn render_with_ordering(
        &mut self,
        data: OutputData,
        _buffer: Option<&VecDeque<OutputData>>,
    ) -> Result<()> {
        let section_key = self.get_section_key(&data);

        if let Some(key) = section_key {
            if key == "live_stack_events" {
                // Special case: streaming section
                self.handle_live_events_data(data).await?;
            } else if key == "confirmation" || key.starts_with("confirmation_") {
                // Special case: confirmation prompts need immediate handling
                self.render_data_immediately(data).await?;
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

    async fn render_section(&mut self, data: OutputData) -> Result<()> {
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }
        self.suppress_main_heading = true;
        self.render_data_immediately(data).await?;
        self.suppress_main_heading = false;
        Ok(())
    }

    fn start_next_section_if_exists(&mut self) {
        if self.next_section_index < self.expected_sections.len() {
            self.start_next_section();
        } else {
            self.cleanup_operation();
        }
    }

    async fn flush_buffered_live_events(&mut self) -> Result<()> {
        let events_to_render: Vec<OutputData> = self.buffered_live_events.drain(..).collect();
        for buffered_event in events_to_render {
            self.render_data_immediately(buffered_event).await?;
        }
        Ok(())
    }

    async fn handle_live_events_data(&mut self, data: OutputData) -> Result<()> {
        self.advance_through_ready_sections().await?;

        // Find the index of live_stack_events in expected sections
        let live_events_index = self
            .expected_sections
            .iter()
            .position(|section| *section == "live_stack_events");

        if let Some(target_index) = live_events_index {
            // If we haven't reached the live_stack_events section yet, buffer the data
            if self.next_section_index < target_index {
                self.buffered_live_events.push(data);
                return Ok(());
            }

            // We're at the live_stack_events section
            if self.next_section_index == target_index {
                if !self.has_section_already_started("live_stack_events") {
                    self.start_next_section();
                    self.flush_buffered_live_events().await?;
                }
                self.render_data_immediately(data).await?;
            }
        }

        Ok(())
    }

    async fn handle_non_section_data(&mut self, data: OutputData) -> Result<()> {
        match data {
            OutputData::OperationComplete(ref info) => {
                // Advance past live_stack_events if we're there
                if self.next_section_index < self.expected_sections.len()
                    && self.expected_sections[self.next_section_index] == "live_stack_events"
                {
                    self.next_section_index += 1;
                }

                if info.skip_remaining_sections {
                    self.cleanup_operation();
                } else {
                    self.advance_through_ready_sections().await?;
                }
            }
            _ => {
                self.render_data_immediately(data).await?;
            }
        }
        Ok(())
    }

    fn has_section_already_started(&self, section_key: &str) -> bool {
        let section_title = self.get_section_title(section_key);
        self.printed_sections.contains(&section_title.to_string())
    }

    fn get_section_key(&self, data: &OutputData) -> Option<String> {
        match data {
            OutputData::CommandMetadata(..) => Some("command_metadata".to_string()),
            OutputData::StackDefinition(..) => Some("stack_definition".to_string()),
            OutputData::StackEvents(..) => Some("stack_events".to_string()),
            OutputData::StackContents(..) => Some("stack_contents".to_string()),
            OutputData::StackList(..) => Some("stack_list".to_string()),
            OutputData::StackDrift(..) => Some("stack_drift".to_string()),
            OutputData::ChangeSetResult(..) => Some("changeset_result".to_string()),
            OutputData::NewStackEvents(..) => Some("live_stack_events".to_string()),
            OutputData::StackChangeDetails(..) => Some("stack_change_details".to_string()),
            OutputData::CostEstimate(..) => Some("cost_estimate".to_string()),
            OutputData::StackTemplate(..) => Some("stack_template".to_string()),
            OutputData::ApprovalRequestResult(..) => Some("approval_request_result".to_string()),
            OutputData::TemplateValidation(..) => Some("template_validation".to_string()),
            OutputData::ApprovalStatus(..) => Some("approval_status".to_string()),
            OutputData::TemplateDiff(..) => Some("template_diff".to_string()),
            OutputData::ApprovalResult(..) => Some("approval_result".to_string()),
            OutputData::ConfirmationPrompt(request) => match &request.key {
                Some(key) => Some(format!("confirmation_{key}")),
                None => Some("confirmation".to_string()),
            },
            // Only non-section data returns None
            OutputData::TokenInfo(..)
            | OutputData::OperationComplete(..)
            | OutputData::InactivityTimeout(..) => None,
            _ => None,
        }
    }

    /// Clean up operation state
    fn cleanup_operation(&mut self) {
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
            OutputData::CommandMetadata(ref metadata) => {
                self.render_command_metadata(metadata).await
            }
            OutputData::StackDefinition(ref def, show_times) => {
                self.render_stack_definition(def, show_times).await
            }
            OutputData::StackEvents(ref events) => self.render_stack_events(events).await,
            OutputData::StackContents(ref contents) => self.render_stack_contents(contents).await,
            OutputData::StatusUpdate(ref update) => self.render_status_update(update).await,
            OutputData::CommandResult(ref result) => self.render_command_result(result).await,
            OutputData::FinalCommandSummary(ref summary) => {
                self.render_final_command_summary(summary).await
            }
            OutputData::StackList(ref list) => self.render_stack_list(list).await,
            OutputData::ChangeSetResult(ref result) => self.render_changeset_result(result).await,
            OutputData::StackDrift(ref drift) => self.render_stack_drift(drift).await,
            OutputData::Error(ref error) => self.render_error(error).await,
            OutputData::TokenInfo(ref token) => self.render_token_info(token).await,
            OutputData::NewStackEvents(ref events) => self.render_new_stack_events(events).await,
            OutputData::OperationComplete(ref info) => self.handle_operation_complete(info).await,
            OutputData::InactivityTimeout(ref info) => self.handle_inactivity_timeout(info).await,
            OutputData::ConfirmationPrompt(request) => {
                self.render_confirmation_prompt(request).await
            }
            OutputData::StackChangeDetails(ref details) => {
                self.render_stack_change_details(details).await
            }
            OutputData::StackAbsentInfo(ref info) => self.render_stack_absent_info(info).await,
            OutputData::CostEstimate(ref estimate) => self.render_cost_estimate(estimate).await,
            OutputData::StackTemplate(ref template) => self.render_stack_template(template).await,
            OutputData::ApprovalRequestResult(ref result) => {
                self.render_approval_request_result(result).await
            }
            OutputData::TemplateValidation(ref validation) => {
                self.render_template_validation(validation).await
            }
            OutputData::ApprovalStatus(ref status) => self.render_approval_status(status).await,
            OutputData::TemplateDiff(ref diff) => self.render_template_diff(diff).await,
            OutputData::ApprovalResult(ref result) => self.render_approval_result(result).await,
        }
    }
}

impl InteractiveRenderer {
    fn render_single_stack_event(
        &self,
        event_with_timing: &StackEventWithTiming,
        status_padding: usize,
        resource_type_padding: usize,
    ) {
        let event = &event_with_timing.event;

        let timestamp = if let Some(ts) = &event.timestamp {
            self.format_timestamp(&self.render_event_timestamp(ts))
        } else {
            self.format_timestamp("                         ")
        };

        // apply padding before coloring to avoid ANSI length issues
        let status_padded = format!("{:<width$}", event.resource_status, width = status_padding);
        let status = if self.are_colors_enabled() {
            self.colorize_resource_status(&status_padded, None)
        } else {
            status_padded
        };

        // Format resource type with padding (plain white for events, not muted)
        let resource_type_padded = format!(
            "{:<width$}",
            event.resource_type,
            width = resource_type_padding
        );
        let resource_type = if self.are_colors_enabled() {
            resource_type_padded.color(self.theme.info).to_string() // Use info color (white) for events
        } else {
            resource_type_padded
        };

        // Format logical ID (no padding needed as it's the last column before duration)
        let logical_id = self.format_logical_id(&event.logical_resource_id);

        let duration_text = if let Some(duration) = event_with_timing.duration_seconds {
            format!(" ({duration}s)")
        } else {
            String::new()
        };

        println!(
            " {} {} {} {}{}",
            timestamp,
            status,
            resource_type,
            logical_id,
            duration_text.color(self.theme.muted)
        );

        // Show resource status reason on new line for failed events
        if let Some(reason) = &event.resource_status_reason
            && !reason.is_empty()
            && event.resource_status.contains("FAILED")
        {
            // TODO: review: Remove ".*Initiated" from reason like iidy-js does
            let cleaned_reason = reason.replace("Initiated", "").trim().to_string();
            if !cleaned_reason.is_empty() {
                let indent = "  ";

                // Wrap long messages at terminal width
                let max_width = self.terminal_width.saturating_sub(2); // Account for indent
                let wrapped_lines = textwrap::wrap(&cleaned_reason, max_width);

                for line in wrapped_lines {
                    println!("{}{}", indent, line.to_string().color(self.theme.error));
                }
            }
        }
    }

    async fn render_command_metadata(&mut self, data: &CommandMetadata) -> Result<()> {
        if !self.suppress_main_heading {
            self.print_section_heading_with_newline("Command Metadata:");
        }

        if let Some(ref cli) = self.cli_context {
            let operation = cli.command.to_cfn_operation();
            self.print_section_entry(
                "CFN Operation:",
                &operation.to_string().color(self.theme.primary).to_string(),
            )?;
        }
        self.print_section_entry(
            "iidy Environment:",
            &data.iidy_environment.color(self.theme.primary).to_string(),
        )?;
        self.print_section_entry(
            "Region:",
            &data.region.color(self.theme.primary).to_string(),
        )?;

        if let Some(profile) = &data.profile
            && !profile.is_empty()
        {
            self.print_section_entry("Profile:", &profile.color(self.theme.primary).to_string())?;
        }

        let service_role = data.iam_service_role.as_deref().unwrap_or("None");
        self.print_section_entry(
            "IAM Service Role:",
            &service_role.color(self.theme.primary).to_string(),
        )?;

        self.print_section_entry(
            "Current IAM Principal:",
            &data
                .current_iam_principal
                .color(self.theme.primary)
                .to_string(),
        )?;
        self.print_section_entry(
            "Credential Source:",
            &data.credential_source.color(self.theme.muted).to_string(),
        )?;

        let cli_args = self.pretty_format_small_map(&data.cli_arguments);
        self.print_section_entry(
            "CLI Arguments:",
            &cli_args.color(self.theme.muted).to_string(),
        )?;

        self.print_section_entry(
            "iidy Version:",
            &data.iidy_version.color(self.theme.muted).to_string(),
        )?;

        self.print_section_entry(
            "Client Req Token:",
            &format!(
                "{} {}",
                data.primary_token.value.color(self.theme.muted),
                format!("({})", self.format_token_source(&data.primary_token.source))
                    .color(self.theme.muted)
            ),
        )?;

        if !data.derived_tokens.is_empty() {
            self.print_section_entry(
                "Derived Tokens:",
                &format!("{} tokens", data.derived_tokens.len()),
            )?;
            for (i, token) in data.derived_tokens.iter().enumerate() {
                self.print_section_entry(
                    &format!("  [{}]", i + 1),
                    &format!(
                        "{} {}",
                        token.value.color(self.theme.muted),
                        format!("({})", self.format_token_source(&token.source))
                            .color(self.theme.muted)
                    ),
                )?;
            }
        }

        Ok(())
    }

    async fn render_stack_definition(
        &mut self,
        data: &StackDefinition,
        show_times: bool,
    ) -> Result<()> {
        if !self.suppress_main_heading {
            self.print_section_heading_with_newline("Stack Details");
        }
        // Note: When suppress_main_heading is true, the heading was already printed with newline
        // by the spinner logic since Stack Details is always multi-line

        if let Some(stackset_name) = data.tags.get("StackSetName") {
            self.print_section_entry(
                "Name (StackSet):",
                &format!(
                    "{} {}",
                    data.name.color(self.theme.muted),
                    stackset_name.color(self.theme.primary)
                ),
            )?;
        } else {
            self.print_section_entry("Name:", &data.name.color(self.theme.primary).to_string())?;
        }

        if let Some(description) = &data.description {
            let description_color = if data.name.starts_with("StackSet") {
                description.color(self.theme.primary).to_string()
            } else {
                description.color(self.theme.muted).to_string()
            };
            self.print_section_entry("Description:", &description_color)?;
        }

        let status_display = if let Some(ref reason) = data.status_reason {
            if !reason.is_empty()
                && (data.status.contains("FAILED")
                    || data.status == "ROLLBACK_COMPLETE"
                    || data.status == "UPDATE_ROLLBACK_COMPLETE")
            {
                format!(
                    "{} {}",
                    self.colorize_resource_status(&data.status, None),
                    reason.color(self.theme.muted)
                )
            } else {
                self.colorize_resource_status(&data.status, None)
            }
        } else {
            self.colorize_resource_status(&data.status, None)
        };
        self.print_section_entry("Status:", &status_display)?;

        let capabilities = if data.capabilities.is_empty() {
            "None".to_string()
        } else {
            data.capabilities.join(", ")
        };
        self.print_section_entry(
            "Capabilities:",
            &capabilities.color(self.theme.muted).to_string(),
        )?;

        let service_role = data.service_role.as_deref().unwrap_or("None");
        self.print_section_entry(
            "Service Role:",
            &service_role.color(self.theme.muted).to_string(),
        )?;

        self.print_section_entry(
            "Region:",
            &data.region.color(self.theme.primary).to_string(),
        )?;

        let tags_str = self.pretty_format_tags(&data.tags);
        self.print_section_entry("Tags:", &tags_str.color(self.theme.muted).to_string())?;

        let params_str = self.pretty_format_parameters(&data.parameters);
        self.print_section_entry(
            "Parameters:",
            &params_str.color(self.theme.muted).to_string(),
        )?;

        self.print_section_entry(
            "DisableRollback:",
            &data
                .disable_rollback
                .to_string()
                .color(self.theme.muted)
                .to_string(),
        )?;

        let protection_text = format!(
            "{}{}",
            data.termination_protection
                .to_string()
                .color(self.theme.muted),
            if data.termination_protection {
                " 🔒"
            } else {
                ""
            }
        );
        self.print_section_entry("TerminationProtection:", &protection_text)?;

        if show_times {
            if let Some(creation_time) = &data.creation_time {
                self.print_section_entry(
                    "Creation Time:",
                    &self
                        .render_timestamp(creation_time)
                        .color(self.theme.muted)
                        .to_string(),
                )?;
            }
            if let Some(last_updated_time) = &data.last_updated_time {
                self.print_section_entry(
                    "Last Update Time:",
                    &self
                        .render_timestamp(last_updated_time)
                        .color(self.theme.muted)
                        .to_string(),
                )?;
            }
        }

        if let Some(timeout) = data.timeout_in_minutes {
            self.print_section_entry(
                "Timeout In Minutes:",
                &timeout.to_string().color(self.theme.muted).to_string(),
            )?;
        }

        let notification_arns = if data.notification_arns.is_empty() {
            "None".to_string()
        } else {
            data.notification_arns.join(", ")
        };
        self.print_section_entry(
            "NotificationARNs:",
            &notification_arns.color(self.theme.muted).to_string(),
        )?;

        if let Some(policy) = &data.stack_policy {
            self.print_section_entry(
                "Stack Policy Source:",
                &policy.color(self.theme.muted).to_string(),
            )?;
        }

        self.print_section_entry("ARN:", &data.arn.color(self.theme.muted).to_string())?;

        self.print_section_entry(
            "Console URL:",
            &data.console_url.color(self.theme.muted).to_string(),
        )?;

        Ok(())
    }

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

        let (events_to_show, truncation_info) = data.get_sorted_limited_events();

        let status_padding = self.calc_padding(&events_to_show, |e| &e.event.resource_status);
        let resource_type_padding = self.calc_padding(&events_to_show, |e| &e.event.resource_type);

        for event_with_timing in &events_to_show {
            self.render_single_stack_event(
                event_with_timing,
                status_padding,
                resource_type_padding,
            );
        }

        if let Some(truncation) = &truncation_info {
            println!(
                "  {}",
                format!(
                    "showing {} of {} events",
                    truncation.shown, truncation.total
                )
                .color(self.theme.muted)
            );
        }

        Ok(())
    }

    async fn render_stack_contents(&mut self, data: &StackContents) -> Result<()> {
        // TODO fix the handling of titles in here.
        if !data.resources.is_empty() {
            if !self.suppress_main_heading {
                self.print_section_heading_with_newline("Stack Resources");
            }
            // Note: When suppress_main_heading is true, the heading was already printed with newline
            // by the spinner logic since Stack Resources is always multi-line
            let id_padding = self.calc_padding(&data.resources, |r| &r.logical_resource_id);
            let resource_type_padding = self.calc_padding(&data.resources, |r| &r.resource_type);

            for resource in &data.resources {
                println!(
                    "{} {} {}",
                    self.format_logical_id(&format!(
                        " {:<width$}",
                        resource.logical_resource_id,
                        width = id_padding
                    )),
                    format!(
                        "{:<width$}",
                        resource.resource_type,
                        width = resource_type_padding
                    )
                    .color(self.theme.muted),
                    resource
                        .physical_resource_id
                        .as_deref()
                        .unwrap_or("")
                        .color(self.theme.muted)
                );
            }
        }

        if data.outputs.is_empty() {
            self.print_section_heading("Stack Outputs");
            println!(" {}", "None".color(self.theme.muted));
        } else {
            self.print_section_heading_with_newline("Stack Outputs");
            let output_key_padding = self.calc_padding(&data.outputs, |o| &o.output_key);

            for output in &data.outputs {
                println!(
                    "{} {}",
                    self.format_logical_id(&format!(
                        " {:<width$}",
                        output.output_key,
                        width = output_key_padding
                    )),
                    output.output_value.color(self.theme.muted)
                );
            }
        }

        if !data.exports.is_empty() {
            // Check if we have importing stacks or multiple exports - if so, use multi-line
            let has_imports = data
                .exports
                .iter()
                .any(|export| !export.importing_stacks.is_empty());
            let is_complex = data.exports.len() > 1 || has_imports;

            if is_complex {
                self.print_section_heading_with_newline("Stack Exports");
            } else {
                self.print_section_heading("Stack Exports");
            }

            let export_name_padding = self.calc_padding(&data.exports, |ex| &ex.name);

            for export in &data.exports {
                println!(
                    "{} {}",
                    self.format_logical_id(&format!(
                        " {:<width$}",
                        export.name,
                        width = export_name_padding
                    )),
                    export.value.color(self.theme.muted)
                );

                for import in &export.importing_stacks {
                    println!(
                        "  {}",
                        format!("imported by {import}").color(self.theme.muted)
                    );
                }
            }
        }

        self.print_section_heading("Current Stack Status");
        println!(
            " {} {}",
            self.colorize_resource_status(&data.current_status.status, None),
            data.current_status
                .status_reason
                .as_deref()
                .unwrap_or("")
                .color(self.theme.muted)
        );

        if !data.pending_changesets.is_empty() {
            self.print_section_heading_with_newline("Pending Changesets");

            for changeset in &data.pending_changesets {
                self.print_section_entry(
                    &self.format_timestamp(&if let Some(ct) = &changeset.creation_time {
                        self.render_timestamp(ct)
                    } else {
                        "Unknown".to_string()
                    }),
                    &format!(
                        "{} {} {}",
                        changeset.change_set_name.color(self.theme.primary),
                        changeset.status,
                        changeset
                            .status_reason
                            .as_deref()
                            .unwrap_or("")
                            .color(self.theme.muted)
                    ),
                )?;

                if let Some(description) = &changeset.description
                    && !description.is_empty()
                {
                    println!("  Description: {}", description.color(self.theme.muted));
                    println!();
                }

                for change in &changeset.changes {
                    self.render_changeset_change(change)?;
                }

                println!();
            }
        }

        Ok(())
    }

    async fn render_status_update(&mut self, data: &StatusUpdate) -> Result<()> {
        let timestamp = if self.options.show_timestamps {
            format!(
                "{} ",
                self.format_timestamp(&self.render_timestamp(&data.timestamp))
            )
        } else {
            String::new()
        };

        let message = match data.level {
            StatusLevel::Error => data.message.color(self.theme.error).to_string(),
            StatusLevel::Warning => {
                if self.are_colors_enabled() {
                    data.message.color(self.theme.warning).to_string()
                } else {
                    data.message.to_string()
                }
            }
            StatusLevel::Info => data.message.to_string(),
            StatusLevel::Success => data.message.color(self.theme.success).to_string(),
        };

        println!("{timestamp}{message}");
        Ok(())
    }

    async fn render_command_result(&mut self, data: &CommandResult) -> Result<()> {
        self.add_content_spacing();

        let status_text = if data.success {
            self.format_section_heading("SUCCESS")
        } else {
            format!("{}:", "FAILURE".color(self.theme.error))
        };

        println!("{} ({}s)", status_text, data.elapsed_seconds);

        if let Some(message) = &data.message {
            println!("{message}");
        }

        Ok(())
    }

    async fn render_final_command_summary(
        &mut self,
        data: &crate::output::data::FinalCommandSummary,
    ) -> Result<()> {
        use crate::output::data::CommandSummaryResult;

        self.add_content_spacing();

        let summary_text = match data.result {
            CommandSummaryResult::Success => {
                if self.are_colors_enabled() {
                    format!("{} 👍", "Success".on_green().black())
                } else {
                    "Success 👍".to_string()
                }
            }
            CommandSummaryResult::Failure => {
                if self.are_colors_enabled() {
                    format!("{} (╯°□°）╯︵ ┻━┻", "Failure".on_red().white())
                } else {
                    "Failure (╯°□°）╯︵ ┻━┻".to_string()
                }
            }
        };
        self.print_section_entry("Command Summary:", &summary_text)?;

        if matches!(data.result, CommandSummaryResult::Failure) {
            println!("Fix and try again.");
        }

        Ok(())
    }

    async fn render_stack_list(&mut self, data: &StackListDisplay) -> Result<()> {
        if data.stacks.is_empty() {
            println!("No stacks found");
            return Ok(());
        }

        const TIME_PADDING: usize = 24;
        let status_padding = self.calc_padding(&data.stacks, |s| &s.stack_status);

        let header = if data.show_tags {
            format!(
                "{:<width1$} {:<width2$} Name, Tags",
                "Creation/Update Time,",
                "Status,",
                width1 = TIME_PADDING,
                width2 = status_padding
            )
        } else {
            format!(
                "{:<width1$} {:<width2$} Name",
                "Creation/Update Time,",
                "Status,",
                width1 = TIME_PADDING,
                width2 = status_padding
            )
        };
        println!("{}", header.color(self.theme.muted));

        for stack in &data.stacks {
            let lifecycle_icon = if stack.termination_protection
                || stack.tags.get("lifetime") == Some(&"protected".to_string())
            {
                "🔒 "
            } else if stack.tags.get("lifetime") == Some(&"long".to_string()) {
                "∞ "
            } else if stack.tags.get("lifetime") == Some(&"short".to_string()) {
                "♺ "
            } else {
                ""
            };

            let base_stack_name = if stack.stack_name.starts_with("StackSet-") {
                format!(
                    "{} {}",
                    stack.stack_name.color(self.theme.muted),
                    stack
                        .tags
                        .get("StackSetName")
                        .unwrap_or(&"Unknown stack set instance".to_string())
                )
            } else {
                stack.stack_name.clone()
            };

            let env_name = if stack.stack_name.contains("production")
                || stack.tags.get("environment") == Some(&"production".to_string())
            {
                "production"
            } else if stack.stack_name.contains("integration")
                || stack.tags.get("environment") == Some(&"integration".to_string())
            {
                "integration"
            } else if stack.stack_name.contains("development")
                || stack.tags.get("environment") == Some(&"development".to_string())
            {
                "development"
            } else {
                ""
            };

            let stack_name = self.color_by_environment(&base_stack_name, env_name);

            let timestamp = if let Some(time) = &stack.last_updated_time {
                format!(
                    "{:>width$}",
                    self.render_event_timestamp(time),
                    width = TIME_PADDING
                )
            } else if let Some(time) = &stack.creation_time {
                format!(
                    "{:>width$}",
                    self.render_event_timestamp(time),
                    width = TIME_PADDING
                )
            } else {
                format!("{:>width$}", "Unknown", width = TIME_PADDING)
            };

            let tags_display = if data.show_tags {
                // Use truncated tags for list view - show Environment tag first and limit to 3 total tags
                format!(
                    " {}",
                    self.pretty_format_tags_with_truncation(&stack.tags, Some(3))
                        .color(self.theme.muted)
                )
            } else {
                String::new()
            };

            // Format status with proper padding (avoid ANSI color issues)
            let status_padded = format!("{:<width$}", stack.stack_status, width = status_padding);
            let status_colored = if self.are_colors_enabled() {
                self.colorize_resource_status(&status_padded, None)
            } else {
                status_padded
            };

            println!(
                "{} {} {}{}{}",
                self.format_timestamp(&timestamp),
                status_colored,
                lifecycle_icon.color(self.theme.muted),
                stack_name,
                tags_display
            );

            if (stack.stack_status.contains("FAILED")
                || stack.stack_status == "ROLLBACK_COMPLETE"
                || stack.stack_status == "UPDATE_ROLLBACK_COMPLETE")
                && let Some(reason) = &stack.status_reason
                && !reason.is_empty()
            {
                println!("  {}", reason.color(self.theme.muted));
            }
        }

        Ok(())
    }

    async fn render_changeset_result(&mut self, data: &ChangeSetCreationResult) -> Result<()> {
        if !self.suppress_main_heading {
            self.add_content_spacing();
        }

        println!();
        println!(
            "AWS Console URL for full changeset review: {}",
            data.console_url.color(self.theme.muted)
        );

        println!();
        self.print_section_heading_with_newline("Pending Changesets");

        for changeset in &data.pending_changesets {
            let creation_time = if let Some(ct) = &changeset.creation_time {
                self.render_timestamp(ct)
            } else {
                "Unknown time".to_string()
            };

            self.print_section_entry(
                &self.format_timestamp(&creation_time),
                &format!(
                    "{} {}",
                    changeset.change_set_name.color(self.theme.primary),
                    changeset.status
                ),
            )?;

            for change in &changeset.changes {
                self.render_changeset_change(change)?;
            }
        }

        println!();
        for step in &data.next_steps {
            println!("{step}");
        }

        Ok(())
    }

    async fn render_stack_drift(&mut self, data: &StackDrift) -> Result<()> {
        if data.drifted_resources.is_empty() {
            println!("No drift detected. Stack resources are in sync with template.");
        } else {
            self.print_section_heading_with_newline("Drifted Resources");

            let id_padding = data
                .drifted_resources
                .iter()
                .map(|d| d.logical_resource_id.len())
                .max()
                .unwrap_or(0);
            let type_padding = data
                .drifted_resources
                .iter()
                .map(|d| d.resource_type.len())
                .max()
                .unwrap_or(0);

            for drift in &data.drifted_resources {
                println!(
                    " {:<width1$} {:<width2$} {}",
                    drift.logical_resource_id.color(self.theme.resource_id),
                    drift.resource_type.color(self.theme.muted),
                    drift.physical_resource_id.color(self.theme.muted),
                    width1 = id_padding,
                    width2 = type_padding
                );
                println!("  {}", drift.drift_status.color(self.theme.error));

                if !drift.property_differences.is_empty() {
                    // Format as YAML with indentation
                    for diff in &drift.property_differences {
                        println!("   - property_path: {}", diff.property_path);
                        if let Some(expected) = &diff.expected_value {
                            println!("     expected_value: {expected}");
                        }
                        if let Some(actual) = &diff.actual_value {
                            println!("     actual_value: {actual}");
                        }
                        if let Some(diff_type) = &diff.difference_type {
                            println!("     difference_type: {diff_type}");
                        }
                    }
                }
            }
        }
        println!();
        Ok(())
    }

    async fn render_error(&mut self, data: &ErrorInfo) -> Result<()> {
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }

        println!();

        match &data.error_details {
            ErrorDetails::StackAbsent(context) => {
                self.render_stack_absent_error_with_context(context).await?;
            }
            ErrorDetails::Generic(details) => {
                if self.are_colors_enabled() {
                    println!(
                        "{}: {}",
                        "ERROR".bold().bright_red(),
                        data.message.bold().bright_red()
                    );
                } else {
                    println!("ERROR: {}", data.message);
                }

                if let Some(details_text) = details {
                    println!();
                    println!("{details_text}");
                }

                if !data.suggestions.is_empty() {
                    for suggestion in &data.suggestions {
                        if self.are_colors_enabled() {
                            println!("  • {}", suggestion.color(self.theme.muted));
                        } else {
                            println!("  • {suggestion}");
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn render_token_info(&mut self, data: &TokenInfo) -> Result<()> {
        // In interactive mode, only show tokens in debug scenarios
        // For now, we'll keep it simple and not display by default
        // TODO: Add verbosity/debug flag checking
        let _ = data; // Suppress unused parameter warning
        Ok(())
    }

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

        self.stop_live_events_timing();
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }

        // Calculate padding for status and resource type columns (same as stack events)
        let status_padding = self.calc_padding(events, |e| &e.event.resource_status);
        let resource_type_padding = self.calc_padding(events, |e| &e.event.resource_type);

        for event_with_timing in events {
            self.render_single_stack_event(
                event_with_timing,
                status_padding,
                resource_type_padding,
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
                if let Some(latest_event) = events.last()
                    && let Some(event_time) = latest_event.event.timestamp
                {
                    self.update_last_event_time(event_time);
                }
            }
        }

        Ok(())
    }

    async fn handle_operation_complete(&mut self, info: &OperationCompleteInfo) -> Result<()> {
        self.stop_live_events_timing();
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }

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
        self.stop_live_events_timing();
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }
        let timeout_message = format!(
            " Inactivity timeout of {} seconds reached. Stopping watch.",
            info.timeout_seconds
        );
        println!("{}", self.style_muted_text(&timeout_message));
        // This signals that live events are done - advance to next section
        self.advance_to_next_section();

        Ok(())
    }

    /// Advance to the next section (non-recursive)
    fn advance_to_next_section(&mut self) {
        if self.next_section_index < self.expected_sections.len() {
            self.next_section_index += 1;
            self.start_next_section();
        }
    }

    async fn render_confirmation_prompt(&mut self, mut request: ConfirmationRequest) -> Result<()> {
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }

        let confirmed = if !self.options.enable_ansi_features {
            // Plain mode: show message but don't interact
            println!("CONFIRMATION REQUIRED: {}", request.message);
            println!("Use --yes flag to proceed automatically in non-interactive mode");
            false // Always decline in non-interactive mode for safety
        } else {
            use std::io::{self, Write};

            loop {
                print!("? {} (y/N) ", request.message.bold().bright_red());
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                let input = input.trim().to_lowercase();

                match input.as_str() {
                    "y" | "yes" => break true,
                    "n" | "no" | "" => break false, // Empty input defaults to No
                    _ => {
                        println!("Please enter 'y' (yes) or 'n' (no)");
                        continue;
                    }
                }
            }
        };

        // Handle post-confirmation actions based on key BEFORE sending response
        // This ensures renderer state is updated before command handler proceeds
        if confirmed {
            if let Some(key) = &request.key {
                match key.as_str() {
                    "execute_changeset" => self.post_confirmation_execute_changeset(),
                    _ => {
                        self.advance_to_next_section();
                    }
                }
            } else {
                self.advance_to_next_section();
            }
        }

        // Send response back to command handler via channel
        if let Some(response_tx) = request.response_tx.take() {
            let _ = response_tx.send(confirmed);
        }

        Ok(())
    }

    fn post_confirmation_execute_changeset(&mut self) {
        self.cleanup_operation();

        self.expected_sections = vec![
            "command_metadata",
            "stack_definition",
            "stack_events",
            "live_stack_events",
            "stack_contents",
        ];
        self.next_section_index = 0;

        // Configure section titles for execution phase (same as ExecuteChangeset operation)
        self.section_titles.insert(
            "command_metadata".to_string(),
            "Command Metadata:".to_string(),
        );
        self.section_titles
            .insert("stack_definition".to_string(), "Stack Details".to_string());
        self.section_titles.insert(
            "stack_events".to_string(),
            "Previous Stack Events (max 10):".to_string(),
        );
        self.section_titles
            .insert("stack_contents".to_string(), "Stack Resources".to_string());
        self.section_titles.insert(
            "live_stack_events".to_string(),
            "Live Stack Events (2s poll):".to_string(),
        );

        // Don't start next section here - let the normal flow handle it when execution phase begins
        // This prevents spinner conflicts between changeset creation and execution phases
    }

    async fn render_stack_absent_info(
        &mut self,
        info: &crate::output::data::StackAbsentInfo,
    ) -> Result<()> {
        if let Some(spinner) = self.current_spinner.take() {
            spinner.clear();
        }

        let (
            info_prefix,
            stack_name_colored,
            env_colored,
            region_colored,
            account_colored,
            auth_arn_colored,
        ) = if self.are_colors_enabled() {
            (
                "info".color(self.theme.success).bold().to_string(),
                info.stack_name.color(self.theme.info).bold().to_string(),
                info.environment.color(self.theme.primary).to_string(),
                info.region.color(self.theme.primary).to_string(),
                info.account.color(self.theme.primary).to_string(),
                info.auth_arn.color(self.theme.primary).to_string(),
            )
        } else {
            (
                "info".to_string(),
                info.stack_name.clone(),
                info.environment.clone(),
                info.region.clone(),
                info.account.clone(),
                info.auth_arn.clone(),
            )
        };

        println!("{info_prefix} The stack {stack_name_colored} is absent");
        println!("      env = {env_colored}");
        println!("      region = {region_colored}");
        println!("      account = {account_colored}");
        println!("      auth_arn = {auth_arn_colored}.");

        Ok(())
    }

    /// Render stack change details for create-or-update operations
    async fn render_stack_change_details(
        &mut self,
        data: &crate::output::data::StackChangeDetails,
    ) -> Result<()> {
        use crate::cfn::StackChangeType;

        if !self.suppress_main_heading {
            self.print_section_heading_with_newline("Stack Change Details");
        }

        match &data.change_type {
            StackChangeType::Create => {
                println!(" {}", "Creating new stack".color(self.theme.info));
            }
            StackChangeType::UpdateWithChanges { .. } => {
                println!(" {}", "Updating existing stack".color(self.theme.info));
            }
            StackChangeType::UpdateNoChanges => {
                println!(
                    " {}",
                    "No changes detected so no stack update needed.".color(self.theme.success)
                );
                // early exit
                self.cleanup_operation();
            }
        }
        Ok(())
    }

    /// Render a single changeset change
    fn render_changeset_change(&self, change: &ChangeInfo) -> Result<()> {
        // Use padding based on length of "Modify  " (8 characters) to accommodate all actions
        let action_width = 8; // len("Modify  ") - fits "Replace?" which is the longest action
        let logical_id_width = 30;

        match change.action.as_str() {
            "Add" => {
                let action_padded = format!("{:<width$}", change.action, width = action_width);
                println!(
                    "  {} {:<width$} {}",
                    action_padded.color(self.theme.success),
                    change.logical_resource_id,
                    change.resource_type.color(self.theme.muted),
                    width = logical_id_width
                );
            }
            "Remove" => {
                let resource_info = if let Some(ref physical_id) = change.physical_resource_id {
                    format!("{} {}", change.resource_type, physical_id)
                } else {
                    change.resource_type.clone()
                };
                let action_padded = format!("{:<width$}", change.action, width = action_width);
                println!(
                    "  {} {:<width$} {}",
                    action_padded.color(self.theme.error),
                    change.logical_resource_id,
                    resource_info.color(self.theme.muted),
                    width = logical_id_width
                );
            }
            "Modify" => {
                let (action_text, action_color) = if let Some(ref replacement) = change.replacement
                {
                    match replacement.as_str() {
                        "True" => ("Replace", self.theme.error),
                        "Conditional" => ("Replace?", self.theme.error),
                        _ => ("Modify", self.theme.warning),
                    }
                } else {
                    ("Modify", self.theme.warning)
                };

                let resource_info = if let Some(ref physical_id) = change.physical_resource_id {
                    format!("{} {}", change.resource_type, physical_id)
                } else {
                    change.resource_type.clone()
                };

                if change.replacement.is_none() || change.replacement.as_deref() == Some("False") {
                    let scope_text = if let Some(ref scope) = change.scope {
                        scope.join(", ")
                    } else {
                        String::new()
                    };

                    let action_padded = format!("{action_text:<action_width$}");
                    println!(
                        "  {} {:<width$} {} {}",
                        action_padded.color(action_color),
                        change.logical_resource_id,
                        scope_text.color(self.theme.warning),
                        resource_info.color(self.theme.muted),
                        width = logical_id_width
                    );
                } else {
                    let action_padded = format!("{action_text:<action_width$}");
                    println!(
                        "  {} {:<width$} {}",
                        action_padded.color(action_color),
                        change.logical_resource_id,
                        resource_info.color(self.theme.muted),
                        width = logical_id_width
                    );
                }

                // Show details as YAML for Modify operations
                if !change.details.is_empty() {
                    for detail in &change.details {
                        println!(
                            "    {}: {}",
                            detail.target.color(self.theme.muted),
                            detail
                                .change_source
                                .as_deref()
                                .unwrap_or("Unknown")
                                .color(self.theme.muted)
                        );
                    }
                }
            }
            _ => {
                // Unknown action - fallback formatting
                let action_padded = format!("{:<width$}", change.action, width = action_width);
                println!(
                    "  {} {:<width$} {}",
                    action_padded,
                    change.logical_resource_id,
                    change.resource_type.color(self.theme.muted),
                    width = logical_id_width
                );
            }
        }

        Ok(())
    }

    async fn render_cost_estimate(
        &mut self,
        data: &crate::output::data::CostEstimate,
    ) -> Result<()> {
        if !self.suppress_main_heading {
            self.print_section_heading_with_newline("Cost Estimate");
        }
        self.print_section_entry(
            "Stack cost estimator:",
            &data.info.url.color(self.theme.primary).to_string(),
        )?;

        Ok(())
    }

    async fn render_stack_template(
        &mut self,
        data: &crate::output::data::StackTemplate,
    ) -> Result<()> {
        for line in &data.stderr_lines {
            eprintln!("{line}");
        }

        println!("{}", data.template_body);

        Ok(())
    }

    async fn render_stack_absent_error_with_context(
        &mut self,
        context: &StackAbsentInfo,
    ) -> Result<()> {
        let (
            error_prefix,
            stack_name_colored,
            env_colored,
            region_colored,
            account_colored,
            auth_arn_colored,
        ) = if self.are_colors_enabled() {
            (
                "ERROR".color(self.theme.error).bold().to_string(),
                context.stack_name.color(self.theme.info).bold().to_string(),
                context.environment.color(self.theme.primary).to_string(),
                context.region.color(self.theme.primary).to_string(),
                context.account.color(self.theme.primary).to_string(),
                context.auth_arn.color(self.theme.primary).to_string(),
            )
        } else {
            (
                "ERROR".to_string(),
                context.stack_name.clone(),
                context.environment.clone(),
                context.region.clone(),
                context.account.clone(),
                context.auth_arn.clone(),
            )
        };

        println!("{error_prefix} The stack {stack_name_colored} is absent");
        println!("      env = {env_colored}");
        println!("      region = {region_colored}");
        println!("      account = {account_colored}");
        println!("      auth_arn = {auth_arn_colored}.");

        Ok(())
    }

    async fn render_approval_request_result(
        &mut self,
        data: &crate::output::ApprovalRequestResult,
    ) -> Result<()> {
        if data.already_approved {
            println!(
                "{}",
                "👍 Your template has already been approved".color(self.theme.success)
            );
        } else {
            self.print_section_heading_with_newline("Template Approval Request");
            println!(
                "Successfully uploaded template to: {}",
                data.pending_location.color(self.theme.muted)
            );
            println!();
            println!("Approve template with:");
            for step in &data.next_steps {
                println!("  {}", step.color(self.theme.primary));
            }
        }
        Ok(())
    }

    async fn render_template_validation(
        &mut self,
        data: &crate::output::TemplateValidation,
    ) -> Result<()> {
        if !data.enabled {
            return Ok(());
        }

        if !data.errors.is_empty() {
            self.print_section_heading_with_newline("Template Validation Errors");
            for error in &data.errors {
                println!(
                    "{} {}",
                    "✗".color(self.theme.error),
                    error.color(self.theme.error)
                );
            }
        }

        if !data.warnings.is_empty() {
            self.print_section_heading_with_newline("Template Validation Warnings");
            for warning in &data.warnings {
                println!(
                    "{} {}",
                    "⚠".color(self.theme.warning),
                    warning.color(self.theme.warning)
                );
            }
        }

        if data.errors.is_empty() && data.warnings.is_empty() {
            println!(
                "{}",
                "✓ Template validation passed".color(self.theme.success)
            );
        }

        Ok(())
    }

    async fn render_approval_status(&mut self, data: &crate::output::ApprovalStatus) -> Result<()> {
        if data.already_approved {
            println!(
                "{}",
                "👍 The template has already been approved".color(self.theme.success)
            );
        } else {
            self.print_section_heading_with_newline("Approval Status");
            println!(
                "Pending template: {}",
                data.pending_location.color(self.theme.muted)
            );
            if let Some(approved) = &data.approved_location {
                println!("Current approved: {}", approved.color(self.theme.muted));
            } else {
                println!("No previously approved template found");
            }
        }
        Ok(())
    }

    async fn render_template_diff(&mut self, data: &crate::output::TemplateDiff) -> Result<()> {
        if !data.has_changes {
            println!("{}", "Templates are identical".color(self.theme.success));
        } else {
            self.print_section_heading_with_newline("Template Changes");
            print!("{}", data.diff_output);
        }
        Ok(())
    }

    async fn render_approval_result(&mut self, data: &crate::output::ApprovalResult) -> Result<()> {
        if data.approved {
            println!();
            println!(
                "{}",
                "Template has been successfully approved!".color(self.theme.success)
            );
            if let Some(location) = &data.approved_location {
                println!("Approved template: {}", location.color(self.theme.muted));
            }
        } else {
            println!("{}", "Approval cancelled".color(self.theme.warning));
        }
        Ok(())
    }
}
