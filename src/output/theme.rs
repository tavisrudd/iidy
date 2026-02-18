//! Theme support for exact iidy-js color matching
//!
//! This module provides the color theme system that matches iidy-js output exactly,
//! using specific xterm colors and true colors as documented in the implementation spec.

use crate::cli::{ColorChoice, Theme};
use owo_colors::{AnsiColors, DynColors};
use std::io::IsTerminal;

/// Color theme for iidy output matching exact iidy-js colors
#[derive(Debug, Clone)]
pub struct IidyTheme {
    /// Whether colors are enabled
    pub colors_enabled: bool,
    /// Timestamp color - xterm 253 (light gray)
    pub timestamp: DynColors,
    /// Logical resource ID color - xterm 252 (light gray)
    pub resource_id: DynColors,
    /// Section heading color - xterm 255 (white)
    pub section_heading: DynColors,
    /// Muted/secondary text - truecolor(128, 128, 128) or blackBright
    pub muted: DynColors,
    /// Primary values (stack names, operations) - magenta
    pub primary: DynColors,
    /// Success/complete states - green
    pub success: DynColors,
    /// Error/failed states - red
    pub error: DynColors,
    /// Warning/in-progress states - yellow
    pub warning: DynColors,
    /// Info states - white
    pub info: DynColors,
    /// Skipped states - xterm 240 (dark gray)
    pub skipped: DynColors,
    /// Environment colors
    pub env_production: DynColors, // red
    pub env_integration: DynColors, // xterm 75 (blue-ish)
    pub env_development: DynColors, // xterm 194 (yellow-ish)
}

impl IidyTheme {
    /// Create a new theme based on terminal capabilities and theme variant
    pub fn new(theme: Theme, color_choice: ColorChoice) -> Self {
        let colors_enabled = Self::should_use_color(color_choice);

        if !colors_enabled {
            // No colors - all use default
            let default = DynColors::Ansi(AnsiColors::Default);
            return Self {
                colors_enabled,
                timestamp: default,
                resource_id: default,
                section_heading: default,
                muted: default,
                primary: default,
                success: default,
                error: default,
                warning: default,
                info: default,
                skipped: default,
                env_production: default,
                env_integration: default,
                env_development: default,
            };
        }

        // Resolve auto theme
        let actual_theme = match theme {
            Theme::Auto => Theme::Dark, // Default to dark theme like iidy-js
            _ => theme,
        };

        match actual_theme {
            Theme::Dark => Self::dark_theme(),
            Theme::Light => Self::light_theme(),
            Theme::HighContrast => Self::high_contrast_theme(),
            Theme::Auto => unreachable!(),
        }
    }

    /// Dark theme - exact iidy-js colors (default)
    fn dark_theme() -> Self {
        Self {
            colors_enabled: true,
            // Exact iidy-js xterm colors for dark theme
            // Using RGB approximations of xterm colors
            timestamp: DynColors::Rgb(212, 212, 212), // xterm 253
            resource_id: DynColors::Rgb(198, 198, 198), // xterm 252
            section_heading: DynColors::Rgb(238, 238, 238), // xterm 255
            muted: DynColors::Rgb(128, 128, 128),     // blackBright equivalent

            // Standard ANSI colors
            primary: DynColors::Ansi(AnsiColors::Magenta),
            success: DynColors::Ansi(AnsiColors::Green),
            error: DynColors::Ansi(AnsiColors::Red),
            warning: DynColors::Ansi(AnsiColors::Yellow),
            info: DynColors::Ansi(AnsiColors::White),
            skipped: DynColors::Rgb(88, 88, 88), // xterm 240

            // Environment-specific colors
            env_production: DynColors::Ansi(AnsiColors::Red),
            env_integration: DynColors::Rgb(95, 175, 255), // xterm 75
            env_development: DynColors::Rgb(215, 255, 215), // xterm 194
        }
    }

    /// Light theme - adjusted for light backgrounds
    fn light_theme() -> Self {
        Self {
            colors_enabled: true,
            timestamp: DynColors::Rgb(105, 105, 105), // Dim gray
            resource_id: DynColors::Rgb(70, 70, 70),  // Dark gray
            section_heading: DynColors::Ansi(AnsiColors::Black),
            muted: DynColors::Rgb(105, 105, 105),   // Dim gray
            primary: DynColors::Rgb(163, 21, 21),   // Dark red (magenta equivalent)
            success: DynColors::Rgb(34, 139, 34),   // Forest green
            error: DynColors::Rgb(220, 20, 60),     // Crimson
            warning: DynColors::Rgb(255, 140, 0),   // Dark orange
            info: DynColors::Rgb(70, 130, 180),     // Steel blue
            skipped: DynColors::Rgb(169, 169, 169), // Dark gray
            env_production: DynColors::Rgb(220, 20, 60), // Crimson
            env_integration: DynColors::Rgb(70, 130, 180), // Steel blue
            env_development: DynColors::Rgb(218, 165, 32), // Golden rod
        }
    }

    /// High contrast theme for accessibility
    fn high_contrast_theme() -> Self {
        Self {
            colors_enabled: true,
            timestamp: DynColors::Ansi(AnsiColors::BrightWhite),
            resource_id: DynColors::Ansi(AnsiColors::BrightCyan),
            section_heading: DynColors::Ansi(AnsiColors::BrightWhite),
            muted: DynColors::Ansi(AnsiColors::White),
            primary: DynColors::Ansi(AnsiColors::BrightMagenta),
            success: DynColors::Ansi(AnsiColors::BrightGreen),
            error: DynColors::Ansi(AnsiColors::BrightRed),
            warning: DynColors::Ansi(AnsiColors::BrightYellow),
            info: DynColors::Ansi(AnsiColors::BrightWhite),
            skipped: DynColors::Ansi(AnsiColors::BrightBlack),
            env_production: DynColors::Ansi(AnsiColors::BrightRed),
            env_integration: DynColors::Ansi(AnsiColors::BrightBlue),
            env_development: DynColors::Ansi(AnsiColors::BrightYellow),
        }
    }

    /// Check if colors should be used based on color choice and environment
    fn should_use_color(color_choice: ColorChoice) -> bool {
        match color_choice {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                // Respect NO_COLOR environment variable
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
        }
    }
}

impl Default for IidyTheme {
    fn default() -> Self {
        Self::new(Theme::Auto, ColorChoice::Auto)
    }
}

/// Get terminal width with fallback to default
pub fn get_terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(130) // DEFAULT_SCREEN_WIDTH from iidy-js
}
