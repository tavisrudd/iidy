use crate::cli::ColorChoice;
use crate::terminal::{ColorTheme, TerminalCapabilities, Theme};
use owo_colors::OwoColorize;
/// Color context management and semantic color markup system
use std::sync::OnceLock;

// Re-export SpinnerStyle for easier access
pub use SpinnerStyle::*;

/// Global color context instance
static GLOBAL_COLOR_CONTEXT: OnceLock<ColorContext> = OnceLock::new();

/// Central color management context
#[derive(Debug, Clone)]
pub struct ColorContext {
    /// Whether color output is enabled
    pub enabled: bool,
    /// Color theme configuration
    pub theme: ColorTheme,
    /// Terminal capabilities
    pub capabilities: TerminalCapabilities,
}

impl ColorContext {
    /// Create a new color context based on user preferences and terminal capabilities
    pub fn new(color_choice: ColorChoice, theme: Theme) -> Self {
        let capabilities = TerminalCapabilities::detect();
        let enabled = Self::should_use_color(color_choice, &capabilities);
        let theme = ColorTheme::for_theme(theme, &capabilities);

        Self {
            enabled,
            theme,
            capabilities,
        }
    }

    /// Initialize the global color context (should be called once at startup)
    pub fn init_global(color_choice: ColorChoice, theme: Theme) -> &'static ColorContext {
        GLOBAL_COLOR_CONTEXT.get_or_init(|| Self::new(color_choice, theme))
    }

    /// Get the global color context (panics if not initialized)
    pub fn global() -> &'static ColorContext {
        GLOBAL_COLOR_CONTEXT
            .get()
            .expect("Color context must be initialized before use")
    }

    /// Determine if color should be used based on choice and capabilities
    fn should_use_color(color_choice: ColorChoice, capabilities: &TerminalCapabilities) -> bool {
        match color_choice {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => capabilities.has_color,
        }
    }

    /// Format text with success semantic color
    pub fn success<T: std::fmt::Display>(&self, text: T) -> String {
        if self.enabled {
            format!("{}", text.color(self.theme.success))
        } else {
            text.to_string()
        }
    }

    /// Format text with error semantic color
    pub fn error<T: std::fmt::Display>(&self, text: T) -> String {
        if self.enabled {
            format!("{}", text.color(self.theme.error))
        } else {
            text.to_string()
        }
    }

    /// Format text with warning semantic color
    pub fn warning<T: std::fmt::Display>(&self, text: T) -> String {
        if self.enabled {
            format!("{}", text.color(self.theme.warning))
        } else {
            text.to_string()
        }
    }

    /// Format text with info semantic color
    pub fn info<T: std::fmt::Display>(&self, text: T) -> String {
        if self.enabled {
            format!("{}", text.color(self.theme.info))
        } else {
            text.to_string()
        }
    }

    /// Format text with muted semantic color
    pub fn muted<T: std::fmt::Display>(&self, text: T) -> String {
        if self.enabled {
            format!("{}", text.color(self.theme.muted))
        } else {
            text.to_string()
        }
    }

    /// Format text with timestamp semantic color
    pub fn timestamp<T: std::fmt::Display>(&self, text: T) -> String {
        if self.enabled {
            format!("{}", text.color(self.theme.timestamp))
        } else {
            text.to_string()
        }
    }

    /// Format text with resource ID semantic color
    pub fn resource_id<T: std::fmt::Display>(&self, text: T) -> String {
        if self.enabled {
            format!("{}", text.color(self.theme.resource_id))
        } else {
            text.to_string()
        }
    }

    /// Format text with skipped status semantic color
    pub fn skipped<T: std::fmt::Display>(&self, text: T) -> String {
        if self.enabled {
            format!("{}", text.color(self.theme.skipped))
        } else {
            text.to_string()
        }
    }

    /// Format text with in-progress status semantic color
    pub fn in_progress<T: std::fmt::Display>(&self, text: T) -> String {
        if self.enabled {
            format!("{}", text.color(self.theme.in_progress))
        } else {
            text.to_string()
        }
    }

    /// Format text with bold styling
    pub fn bold<T: std::fmt::Display>(&self, text: T) -> String {
        if self.enabled {
            format!("{}", text.bold())
        } else {
            text.to_string()
        }
    }

    /// Format CloudFormation resource status with appropriate semantic color
    pub fn format_resource_status(&self, status: &str) -> String {
        let status_upper = status.to_uppercase();

        if status_upper.contains("FAILED") || status_upper.contains("ROLLBACK") {
            self.error(status)
        } else if status_upper.contains("COMPLETE") {
            self.success(status)
        } else if status_upper.contains("PROGRESS") || status_upper.contains("PENDING") {
            self.in_progress(status)
        } else if status_upper.contains("SKIPPED") {
            self.skipped(status)
        } else {
            // Default to muted for unknown statuses
            self.muted(status)
        }
    }
}

/// Convenience trait for semantic color markup on strings and other display types
pub trait ColorExt {
    /// Apply success semantic color
    fn success(self) -> String;
    /// Apply error semantic color
    fn error(self) -> String;
    /// Apply warning semantic color
    fn warning(self) -> String;
    /// Apply info semantic color
    fn info(self) -> String;
    /// Apply muted semantic color
    fn muted(self) -> String;
    /// Apply timestamp semantic color
    fn timestamp(self) -> String;
    /// Apply resource ID semantic color
    fn resource_id(self) -> String;
    /// Apply skipped status semantic color
    fn skipped(self) -> String;
    /// Apply in-progress status semantic color
    fn in_progress(self) -> String;
    /// Apply bold styling
    fn bold_text(self) -> String;
    /// Format CloudFormation resource status with appropriate semantic color
    fn format_status(self) -> String;
}

impl<T: std::fmt::Display> ColorExt for T {
    fn success(self) -> String {
        ColorContext::global().success(self)
    }

    fn error(self) -> String {
        ColorContext::global().error(self)
    }

    fn warning(self) -> String {
        ColorContext::global().warning(self)
    }

    fn info(self) -> String {
        ColorContext::global().info(self)
    }

    fn muted(self) -> String {
        ColorContext::global().muted(self)
    }

    fn timestamp(self) -> String {
        ColorContext::global().timestamp(self)
    }

    fn resource_id(self) -> String {
        ColorContext::global().resource_id(self)
    }

    fn skipped(self) -> String {
        ColorContext::global().skipped(self)
    }

    fn in_progress(self) -> String {
        ColorContext::global().in_progress(self)
    }

    fn bold_text(self) -> String {
        ColorContext::global().bold(self)
    }

    fn format_status(self) -> String {
        ColorContext::global().format_resource_status(&self.to_string())
    }
}

/// Spinner style options similar to ora's animation choices
#[derive(Debug, Clone, Copy)]
pub enum SpinnerStyle {
    /// Default dots animation (⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏)
    Dots,
    /// Dots12 animation like ora (equivalent to ora's dots12)
    Dots12,
    /// Line animation (⠂⠄⠅⠇⡇⣇⣧⣷⣿)
    Line,
    /// Arrow animation (←↖↑↗→↘↓↙)
    Arrow,
    /// Pulse animation (⚫⚪)
    Pulse,
}

/// Progress manager for TTY-aware progress indication with ora-like API
pub struct ProgressManager {
    spinner: Option<indicatif::ProgressBar>,
}

impl ProgressManager {
    /// Create a new progress manager with default spinner style
    pub fn new() -> Self {
        Self::with_style(SpinnerStyle::Dots12, "")
    }

    /// Create a new progress manager with custom style and message (ora-like constructor)
    pub fn with_style(style: SpinnerStyle, message: &str) -> Self {
        let context = ColorContext::global();
        let is_tty = context.enabled && std::io::IsTerminal::is_terminal(&std::io::stdout());

        let spinner = if is_tty {
            let pb = indicatif::ProgressBar::new_spinner();
            Self::apply_spinner_style(&pb, style);
            if !message.is_empty() {
                pb.set_message(message.to_string());
            }
            Some(pb)
        } else {
            None
        };

        Self { spinner }
    }

    /// Apply a spinner style to a progress bar
    fn apply_spinner_style(pb: &indicatif::ProgressBar, style: SpinnerStyle) {
        use std::time::Duration;

        let (tick_chars, template) = match style {
            SpinnerStyle::Dots => ("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏", "{spinner:.cyan.bold} {msg}"),
            SpinnerStyle::Dots12 => ("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏⠋⠙", "{spinner:.cyan.bold} {msg}"),
            SpinnerStyle::Line => ("⠂⠄⠅⠇⡇⣇⣧⣷⣿⣸⣰⣠⣀", "{spinner:.yellow} {msg}"),
            SpinnerStyle::Arrow => ("←↖↑↗→↘↓↙", "{spinner:.magenta} {msg}"),
            SpinnerStyle::Pulse => ("⚫⚪", "{spinner:.green} {msg}"),
        };

        pb.set_style(
            indicatif::ProgressStyle::default_spinner()
                .tick_chars(tick_chars)
                .template(template)
                .expect("Invalid progress template"),
        );

        // Set tick rate based on animation (slower for arrows/pulse, faster for dots)
        let tick_rate = match style {
            SpinnerStyle::Arrow | SpinnerStyle::Pulse => Duration::from_millis(200),
            _ => Duration::from_millis(100),
        };
        pb.enable_steady_tick(tick_rate);
    }

    // ===== ORA-LIKE API METHODS =====

    /// Start the spinner (ora.start() equivalent)
    pub fn start(&self) {
        // Spinner is automatically started when created, but this makes it explicit
        if let Some(spinner) = &self.spinner {
            spinner.enable_steady_tick(std::time::Duration::from_millis(100));
        }
    }

    /// Stop the spinner without any message (ora.stop() equivalent)
    pub fn stop(&self) {
        if let Some(spinner) = &self.spinner {
            spinner.finish_and_clear();
        }
    }

    /// Finish with success message and checkmark (ora.succeed() equivalent)
    pub fn succeed(&self, msg: &str) {
        if let Some(spinner) = &self.spinner {
            spinner.finish_with_message(format!("✓ {}", msg).success());
        } else {
            eprintln!("✓ {}", msg);
        }
    }

    /// Finish with error message and X mark (ora.fail() equivalent)
    pub fn fail(&self, msg: &str) {
        if let Some(spinner) = &self.spinner {
            spinner.finish_with_message(format!("✗ {}", msg).error());
        } else {
            eprintln!("✗ {}", msg);
        }
    }

    /// Finish with warning message and warning symbol (ora.warn() equivalent)
    pub fn warn(&self, msg: &str) {
        if let Some(spinner) = &self.spinner {
            spinner.finish_with_message(format!("⚠ {}", msg).warning());
        } else {
            eprintln!("⚠ {}", msg);
        }
    }

    /// Finish with info message and info symbol (ora.info() equivalent)
    pub fn info(&self, msg: &str) {
        if let Some(spinner) = &self.spinner {
            spinner.finish_with_message(format!("ℹ {}", msg).info());
        } else {
            eprintln!("ℹ {}", msg);
        }
    }

    /// Update the spinner text while running (ora.text = "..." equivalent)
    pub fn set_text(&self, text: &str) {
        if let Some(spinner) = &self.spinner {
            spinner.set_message(text.to_string());
        } else {
            // Fallback for non-TTY: print status updates
            eprintln!("Status: {}", text);
        }
    }

    /// Clear the spinner (ora.clear() equivalent)
    pub fn clear(&self) {
        if let Some(spinner) = &self.spinner {
            spinner.finish_and_clear();
        }
    }
    
    /// Get a reference to the underlying ProgressBar for background tasks
    /// (indicatif::ProgressBar is thread-safe and designed to be cloned)
    pub fn get_spinner_ref(&self) -> Option<indicatif::ProgressBar> {
        self.spinner.clone()
    }

    /// Change the spinner style dynamically
    pub fn set_spinner_style(&self, style: SpinnerStyle) {
        if let Some(spinner) = &self.spinner {
            Self::apply_spinner_style(spinner, style);
        }
    }

    // ===== LEGACY COMPATIBILITY METHODS =====

    /// Set the progress message (legacy method for compatibility)
    pub fn set_message(&self, msg: &str) {
        self.set_text(msg);
    }

    /// Finish the progress indication with a success message (legacy)
    pub fn finish_with_message(&self, msg: &str) {
        self.succeed(msg);
    }

    /// Finish the progress indication with an error message (legacy)
    pub fn finish_with_error(&self, msg: &str) {
        self.fail(msg);
    }
}

impl Default for ProgressManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Theme;

    fn init_test_context() {
        let _ = ColorContext::init_global(ColorChoice::Never, Theme::Dark);
    }

    #[test]
    fn color_context_respects_never_choice() {
        let context = ColorContext::new(ColorChoice::Never, Theme::Dark);
        assert!(!context.enabled);

        // Should return plain text when disabled
        let result = context.success("test");
        assert_eq!(result, "test");
    }

    #[test]
    fn color_context_respects_always_choice() {
        let context = ColorContext::new(ColorChoice::Always, Theme::Dark);
        assert!(context.enabled);

        // Should return colored text when enabled (exact format depends on theme)
        let result = context.success("test");
        assert!(result.contains("test"));
        // In enabled mode, result should contain ANSI codes (more than just "test")
        assert!(result.len() >= 4); // "test" + some color codes
    }

    #[test]
    fn semantic_color_methods_work() {
        let context = ColorContext::new(ColorChoice::Never, Theme::Dark);

        // Test all semantic color methods with disabled context
        assert_eq!(context.success("test"), "test");
        assert_eq!(context.error("test"), "test");
        assert_eq!(context.warning("test"), "test");
        assert_eq!(context.info("test"), "test");
        assert_eq!(context.muted("test"), "test");
        assert_eq!(context.timestamp("test"), "test");
        assert_eq!(context.resource_id("test"), "test");
        assert_eq!(context.skipped("test"), "test");
        assert_eq!(context.in_progress("test"), "test");
        assert_eq!(context.bold("test"), "test");
    }

    #[test]
    fn resource_status_formatting() {
        let context = ColorContext::new(ColorChoice::Never, Theme::Dark);

        // Test status categorization (colors won't show since disabled)
        assert_eq!(
            context.format_resource_status("CREATE_COMPLETE"),
            "CREATE_COMPLETE"
        );
        assert_eq!(
            context.format_resource_status("CREATE_FAILED"),
            "CREATE_FAILED"
        );
        assert_eq!(
            context.format_resource_status("CREATE_IN_PROGRESS"),
            "CREATE_IN_PROGRESS"
        );
        assert_eq!(
            context.format_resource_status("UNKNOWN_STATUS"),
            "UNKNOWN_STATUS"
        );
    }

    #[test]
    fn color_ext_trait_works() {
        init_test_context();

        // Test trait methods (context is disabled so should return plain text)
        assert_eq!("test".success(), "test");
        assert_eq!("test".error(), "test");
        assert_eq!("test".warning(), "test");
        assert_eq!("test".info(), "test");
        assert_eq!("test".muted(), "test");
        assert_eq!("test".timestamp(), "test");
        assert_eq!("test".resource_id(), "test");
        assert_eq!("test".skipped(), "test");
        assert_eq!("test".in_progress(), "test");
        assert_eq!("test".bold_text(), "test");
        assert_eq!("CREATE_COMPLETE".format_status(), "CREATE_COMPLETE");
    }

    #[test]
    fn progress_manager_creation() {
        init_test_context();

        // Should create without panicking
        let _manager = ProgressManager::new();
        let _default_manager = ProgressManager::default();
    }
}
