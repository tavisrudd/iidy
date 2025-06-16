/// Terminal capabilities detection and color theming support
use std::io::IsTerminal;

/// Terminal color and display capabilities
#[derive(Debug, Clone)]
pub struct TerminalCapabilities {
    /// Whether the terminal supports color output
    pub has_color: bool,
    /// Whether the terminal supports 24-bit true color
    pub has_true_color: bool,
    /// Terminal width in columns, if detectable
    pub width: Option<usize>,
}

impl TerminalCapabilities {
    /// Detect current terminal capabilities based on environment and TTY status
    pub fn detect() -> Self {
        let has_color = Self::detect_color_support();
        let has_true_color = has_color && Self::detect_true_color_support();
        let width = Self::detect_terminal_width();

        Self {
            has_color,
            has_true_color,
            width,
        }
    }

    /// Check if terminal supports color output
    fn detect_color_support() -> bool {
        // Respect NO_COLOR environment variable (https://no-color.org/)
        if std::env::var("NO_COLOR").is_ok() {
            return false;
        }

        // Check FORCE_COLOR environment variable
        if std::env::var("FORCE_COLOR").is_ok() {
            return true;
        }

        // Check if stdout is a TTY
        std::io::stdout().is_terminal()
    }

    /// Check if terminal supports 24-bit true color
    fn detect_true_color_support() -> bool {
        // Check COLORTERM environment variable for true color support
        std::env::var("COLORTERM")
            .map(|v| v == "truecolor" || v == "24bit")
            .unwrap_or(false)
    }

    /// Detect terminal width in columns
    fn detect_terminal_width() -> Option<usize> {
        terminal_size::terminal_size().map(|(w, _)| w.0 as usize)
    }
}

/// Theme variants for color schemes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    /// Auto-detect theme based on terminal type and environment
    Auto,
    /// Light background optimized colors
    Light,
    /// Dark background optimized colors
    Dark,
    /// High contrast colors for accessibility
    HighContrast,
}

impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Theme::Auto => "auto",
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::HighContrast => "high-contrast",
        };
        write!(f, "{}", s)
    }
}

/// Semantic color definitions for consistent theming
#[derive(Debug, Clone)]
pub struct ColorTheme {
    /// Success status color (typically green)
    pub success: owo_colors::DynColors,
    /// Error status color (typically red)
    pub error: owo_colors::DynColors,
    /// Warning status color (typically yellow/orange)
    pub warning: owo_colors::DynColors,
    /// Info status color (typically blue)
    pub info: owo_colors::DynColors,
    /// Muted text color (typically gray)
    pub muted: owo_colors::DynColors,
    /// Timestamp color (typically light gray)
    pub timestamp: owo_colors::DynColors,
    /// Resource identifier color (typically cyan)
    pub resource_id: owo_colors::DynColors,
    /// Skipped status color (typically blue)
    pub skipped: owo_colors::DynColors,
    /// In-progress status color (typically yellow)
    pub in_progress: owo_colors::DynColors,
}

impl ColorTheme {
    /// Create a color theme based on the theme variant and terminal capabilities
    pub fn for_theme(theme: Theme, capabilities: &TerminalCapabilities) -> Self {
        let actual_theme = match theme {
            Theme::Auto => Self::detect_auto_theme(),
            other => other,
        };

        match actual_theme {
            Theme::Light => Self::light_theme(capabilities),
            Theme::Dark => Self::dark_theme(capabilities),
            Theme::HighContrast => Self::high_contrast_theme(capabilities),
            Theme::Auto => unreachable!("Auto theme should be resolved by this point"),
        }
    }

    /// Auto-detect appropriate theme based on environment
    fn detect_auto_theme() -> Theme {
        // Try to detect terminal background
        // Many terminals don't expose this, so default to dark
        // In the future, could check COLORFGBG or other env vars
        Theme::Dark
    }

    /// Light background optimized color theme
    fn light_theme(capabilities: &TerminalCapabilities) -> Self {
        if capabilities.has_true_color {
            Self {
                success: owo_colors::DynColors::Rgb(34, 139, 34), // Forest green
                error: owo_colors::DynColors::Rgb(220, 20, 60),   // Crimson
                warning: owo_colors::DynColors::Rgb(255, 140, 0), // Dark orange
                info: owo_colors::DynColors::Rgb(70, 130, 180),   // Steel blue
                muted: owo_colors::DynColors::Rgb(105, 105, 105), // Dim gray
                timestamp: owo_colors::DynColors::Rgb(128, 128, 128), // Gray
                resource_id: owo_colors::DynColors::Rgb(47, 79, 79), // Dark slate gray
                skipped: owo_colors::DynColors::Rgb(100, 149, 237), // Cornflower blue
                in_progress: owo_colors::DynColors::Rgb(218, 165, 32), // Golden rod
            }
        } else {
            Self {
                success: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::Green),
                error: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::Red),
                warning: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::Yellow),
                info: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::Blue),
                muted: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::Black),
                timestamp: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::Black),
                resource_id: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::Cyan),
                skipped: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::Blue),
                in_progress: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::Yellow),
            }
        }
    }

    /// Dark background optimized color theme
    fn dark_theme(capabilities: &TerminalCapabilities) -> Self {
        if capabilities.has_true_color {
            Self {
                success: owo_colors::DynColors::Rgb(50, 205, 50), // Lime green
                error: owo_colors::DynColors::Rgb(255, 99, 71),   // Tomato
                warning: owo_colors::DynColors::Rgb(255, 165, 0), // Orange
                info: owo_colors::DynColors::Rgb(135, 206, 235),  // Sky blue
                muted: owo_colors::DynColors::Rgb(169, 169, 169), // Dark gray
                timestamp: owo_colors::DynColors::Rgb(192, 192, 192), // Silver
                resource_id: owo_colors::DynColors::Rgb(64, 224, 208), // Turquoise
                skipped: owo_colors::DynColors::Rgb(173, 216, 230), // Light blue
                in_progress: owo_colors::DynColors::Rgb(255, 215, 0), // Gold
            }
        } else {
            Self {
                success: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightGreen),
                error: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightRed),
                warning: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightYellow),
                info: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightBlue),
                muted: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightBlack),
                timestamp: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::White),
                resource_id: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightCyan),
                skipped: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightBlue),
                in_progress: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightYellow),
            }
        }
    }

    /// High contrast theme for accessibility
    fn high_contrast_theme(_capabilities: &TerminalCapabilities) -> Self {
        // Use basic ANSI colors for maximum compatibility and contrast
        Self {
            success: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightGreen),
            error: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightRed),
            warning: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightYellow),
            info: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightBlue),
            muted: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::White),
            timestamp: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightWhite),
            resource_id: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightCyan),
            skipped: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightMagenta),
            in_progress: owo_colors::DynColors::Ansi(owo_colors::AnsiColors::BrightYellow),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_capabilities_detects_basic_properties() {
        let caps = TerminalCapabilities::detect();
        // Basic smoke test - exact values depend on test environment
        // Just ensure we can construct capabilities without panicking
        assert!(caps.width.is_none() || caps.width.unwrap() > 0);
    }

    #[test]
    fn theme_display_formats_correctly() {
        assert_eq!(Theme::Auto.to_string(), "auto");
        assert_eq!(Theme::Light.to_string(), "light");
        assert_eq!(Theme::Dark.to_string(), "dark");
        assert_eq!(Theme::HighContrast.to_string(), "high-contrast");
    }

    #[test]
    fn color_theme_creation_works_for_all_variants() {
        let caps = TerminalCapabilities {
            has_color: true,
            has_true_color: false,
            width: Some(80),
        };

        // Test that all theme variants can be created without panicking
        let _light = ColorTheme::for_theme(Theme::Light, &caps);
        let _dark = ColorTheme::for_theme(Theme::Dark, &caps);
        let _high_contrast = ColorTheme::for_theme(Theme::HighContrast, &caps);
        let _auto = ColorTheme::for_theme(Theme::Auto, &caps);
    }

    #[test]
    fn true_color_affects_theme_creation() {
        let basic_caps = TerminalCapabilities {
            has_color: true,
            has_true_color: false,
            width: Some(80),
        };

        let true_color_caps = TerminalCapabilities {
            has_color: true,
            has_true_color: true,
            width: Some(80),
        };

        let basic_theme = ColorTheme::for_theme(Theme::Dark, &basic_caps);
        let true_color_theme = ColorTheme::for_theme(Theme::Dark, &true_color_caps);

        // Themes should be different when true color is available
        // This is a basic structural test - exact color values may vary
        match (&basic_theme.success, &true_color_theme.success) {
            (owo_colors::DynColors::Ansi(_), owo_colors::DynColors::Rgb(_, _, _)) => {
                // Expected: basic uses ANSI, true color uses RGB
            }
            _ => panic!("Expected different color types for basic vs true color themes"),
        }
    }
}
