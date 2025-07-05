/// Theme demonstration module to showcase color capabilities
use crate::output::color::{ColorExt, ProgressManager};
use crate::output::terminal::{ColorTheme, TerminalCapabilities, Theme};
use std::thread;
use std::time::Duration;

/// Demonstrate all available color themes and terminal capabilities
pub fn run_theme_demo() {
    println!("{}", "iidy Color Theme Demonstration".bold_text());
    println!();

    // Show terminal capabilities
    demonstrate_terminal_capabilities();
    println!();

    // Show all themes
    demonstrate_all_themes();
    println!();

    // Show CloudFormation status colors
    demonstrate_cloudformation_colors();
    println!();

    // Show progress indicator demo
    demonstrate_progress_indicator();
    println!();

    println!("{}", "Theme demonstration complete!".success());
}

fn demonstrate_terminal_capabilities() {
    println!("{}", "Terminal Capabilities:".bold_text());

    let caps = TerminalCapabilities::detect();

    println!(
        "  TTY Support:     {}",
        if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
            "Yes".success()
        } else {
            "No".error()
        }
    );

    println!(
        "  Color Support:   {}",
        if caps.has_color {
            "Yes".success()
        } else {
            "No".error()
        }
    );

    println!(
        "  True Color:      {}",
        if caps.has_true_color {
            "Yes (24-bit)".success()
        } else {
            "No (fallback to ANSI)".warning()
        }
    );

    println!(
        "  Terminal Width:  {}",
        caps.width
            .map(|w| format!("{} columns", w).info())
            .unwrap_or_else(|| "Unknown".muted())
    );

    // Environment variable status
    println!(
        "  NO_COLOR:        {}",
        if std::env::var("NO_COLOR").is_ok() {
            "Set (colors disabled)".warning()
        } else {
            "Not set".muted()
        }
    );

    println!(
        "  FORCE_COLOR:     {}",
        if std::env::var("FORCE_COLOR").is_ok() {
            "Set (colors forced)".info()
        } else {
            "Not set".muted()
        }
    );

    println!(
        "  COLORTERM:       {}",
        std::env::var("COLORTERM")
            .map(|v| v.info())
            .unwrap_or_else(|_| "Not set".muted())
    );
}

fn demonstrate_all_themes() {
    println!("{}", "Available Themes:".bold_text());

    let caps = TerminalCapabilities::detect();
    let themes = vec![
        (Theme::Auto, "Auto (detects environment)"),
        (Theme::Light, "Light (light background optimized)"),
        (Theme::Dark, "Dark (dark background optimized)"),
        (Theme::HighContrast, "High Contrast (accessibility focused)"),
    ];

    for (theme, description) in themes {
        println!();
        println!(
            "  {} - {}",
            format!("{:?}", theme).bold_text(),
            description.muted()
        );

        let color_theme = ColorTheme::for_theme(theme, &caps);
        demonstrate_theme_colors(&color_theme);
    }
}

fn demonstrate_theme_colors(theme: &ColorTheme) {
    use owo_colors::OwoColorize;

    println!(
        "    Success:     {}",
        "✓ Operation completed successfully".color(theme.success)
    );
    println!(
        "    Error:       {}",
        "✗ Operation failed with error".color(theme.error)
    );
    println!(
        "    Warning:     {}",
        "⚠ Warning: potential issue detected".color(theme.warning)
    );
    println!(
        "    Info:        {}",
        "ℹ Information: process started".color(theme.info)
    );
    println!(
        "    Muted:       {}",
        "Additional details and metadata".color(theme.muted)
    );
    println!(
        "    Timestamp:   {}",
        "2025-06-07T12:34:56Z".color(theme.timestamp)
    );
    println!(
        "    Resource ID: {}",
        "MyBucket-1a2b3c4d5e6f".color(theme.resource_id)
    );
    println!(
        "    Skipped:     {}",
        "⊘ Operation skipped (no changes needed)".color(theme.skipped)
    );
    println!(
        "    In Progress: {}",
        "⟳ Operation in progress...".color(theme.in_progress)
    );
}

fn demonstrate_cloudformation_colors() {
    println!("{}", "CloudFormation Status Colors:".bold_text());

    let statuses = vec![
        "CREATE_COMPLETE",
        "CREATE_FAILED",
        "CREATE_IN_PROGRESS",
        "UPDATE_COMPLETE",
        "UPDATE_FAILED",
        "UPDATE_IN_PROGRESS",
        "DELETE_COMPLETE",
        "DELETE_FAILED",
        "DELETE_IN_PROGRESS",
        "ROLLBACK_COMPLETE",
        "ROLLBACK_FAILED",
        "ROLLBACK_IN_PROGRESS",
        "REVIEW_IN_PROGRESS",
        "IMPORT_COMPLETE",
        "IMPORT_IN_PROGRESS",
        "CREATE_PENDING",
        "DELETE_SKIPPED",
        "UNKNOWN_STATUS",
    ];

    println!();
    for status in statuses {
        println!("  {:<25} {}", status.muted(), status.format_status());
    }
}

fn demonstrate_progress_indicator() {
    println!("{}", "Progress Indicator Demo:".bold_text());

    let progress = ProgressManager::new();

    if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        println!("  {} TTY detected - showing spinner", "✓".success());

        let steps = vec![
            "Validating CloudFormation template",
            "Creating change set",
            "Waiting for stack creation",
            "Checking resource status",
            "Finalizing deployment",
        ];

        for (i, step) in steps.iter().enumerate() {
            progress.set_message(&format!("[{}/{}] {}", i + 1, steps.len(), step));
            thread::sleep(Duration::from_millis(800));
        }

        progress.finish_with_message("Stack creation completed successfully");
    } else {
        println!("  {} Non-TTY detected - showing text updates", "ℹ".info());

        let steps = vec![
            "Validating CloudFormation template",
            "Creating change set",
            "Waiting for stack creation",
            "Checking resource status",
            "Finalizing deployment",
        ];

        for (i, step) in steps.iter().enumerate() {
            progress.set_message(&format!("[{}/{}] {}", i + 1, steps.len(), step));
            thread::sleep(Duration::from_millis(500));
        }

        progress.finish_with_message("Stack creation completed successfully");
    }

    println!();
    println!("  {} Progress demonstration complete", "✓".success());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ColorChoice;
    use crate::output::color::ColorContext;

    fn init_test_context() {
        let _ = ColorContext::init_global(ColorChoice::Never, Theme::Dark);
    }

    #[test]
    fn theme_demo_runs_without_panic() {
        init_test_context();

        // Should run without panicking even in test environment
        // We can't easily test the actual output, but we can ensure it doesn't crash
        run_theme_demo();
    }

    #[test]
    fn terminal_capabilities_demo_works() {
        init_test_context();
        demonstrate_terminal_capabilities();
    }

    #[test]
    fn all_themes_demo_works() {
        init_test_context();
        demonstrate_all_themes();
    }

    #[test]
    fn cloudformation_colors_demo_works() {
        init_test_context();
        demonstrate_cloudformation_colors();
    }
}
