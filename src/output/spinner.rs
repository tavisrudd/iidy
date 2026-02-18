use std::io::IsTerminal;

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

/// Spinner manager for TTY-aware progress indication
pub struct Spinner {
    spinner: Option<indicatif::ProgressBar>,
}

impl Spinner {
    /// Create a new spinner with custom style and message
    pub fn with_style(style: SpinnerStyle, message: &str) -> Self {
        let is_tty = std::io::stdout().is_terminal();

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

    /// Clear the spinner
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
}
