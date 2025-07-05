//! Terminal background detection proof of concept
//! 
//! Demonstrates the various strategies for detecting terminal background
//! colors and validates the theme auto-detection logic.

use crate::cli::Theme;
use crate::output::terminal::{ColorTheme, TerminalCapabilities, Theme as TerminalTheme};
use std::collections::HashMap;
use std::io::IsTerminal;

/// Comprehensive terminal background detection demo
pub fn run_detect_background_demo(explicit_theme: Option<Theme>) {
    println!("🔍 Terminal Background Detection Demo");
    println!("====================================\n");

    // Show terminal capabilities
    show_terminal_capabilities();
    
    // Show environment variables
    show_environment_analysis();
    
    // Test detection strategies
    test_detection_strategies();
    
    // Final recommendation
    show_final_recommendation();
    
    // Theme override demonstration
    demonstrate_theme_override(explicit_theme);
}

fn show_terminal_capabilities() {
    println!("📟 Terminal Capabilities:");
    println!("  TTY:              {}", std::io::stdout().is_terminal());
    println!("  TERM:             {}", std::env::var("TERM").unwrap_or_else(|_| "not set".to_string()));
    println!("  COLORTERM:        {}", std::env::var("COLORTERM").unwrap_or_else(|_| "not set".to_string()));
    println!("  NO_COLOR:         {}", if std::env::var("NO_COLOR").is_ok() { "set (colors disabled)" } else { "not set" });
    
    if let Some((width, height)) = terminal_size::terminal_size() {
        println!("  Terminal Size:    {}x{}", width.0, height.0);
    } else {
        println!("  Terminal Size:    unknown");
    }
    
    println!();
}

fn show_environment_analysis() {
    println!("🌍 Environment Variable Analysis:");
    
    // Check key variables for background detection
    let env_vars = [
        ("COLORFGBG", "Terminal foreground/background colors"),
        ("TERM_PROGRAM", "Terminal emulator identifier"),
        ("TERM_PROGRAM_VERSION", "Terminal emulator version"),
        ("ITERM_SESSION_ID", "iTerm2 session indicator"),
        ("VSCODE_INJECTION", "VS Code integrated terminal"),
        ("DARK_MODE", "Explicit dark mode setting"),
        ("THEME", "Explicit theme setting"),
        ("BACKGROUND", "Explicit background setting"),
    ];
    
    for (var, description) in &env_vars {
        let value = std::env::var(var).unwrap_or_else(|_| "not set".to_string());
        println!("  {:<20} {} -> {}", var, description, value);
    }
    
    println!();
}

fn test_detection_strategies() {
    println!("🧪 Detection Strategy Tests:");
    
    let mut results = HashMap::new();
    
    // Strategy 1: COLORFGBG analysis
    let colorfgbg_result = detect_via_colorfgbg();
    results.insert("COLORFGBG", colorfgbg_result);
    println!("  1. COLORFGBG Analysis:    {}", format_result(colorfgbg_result));
    
    // Strategy 2: Terminal program detection
    let term_program_result = detect_via_term_program();
    results.insert("TERM_PROGRAM", term_program_result);
    println!("  2. Terminal Program:      {}", format_result(term_program_result));
    
    // Strategy 3: Theme environment variables
    let theme_vars_result = detect_via_theme_vars();
    results.insert("THEME_VARS", theme_vars_result);
    println!("  3. Theme Variables:       {}", format_result(theme_vars_result));
    
    // Strategy 4: Terminal-specific indicators
    let specific_indicators_result = detect_via_specific_indicators();
    results.insert("SPECIFIC", specific_indicators_result);
    println!("  4. Specific Indicators:   {}", format_result(specific_indicators_result));
    
    println!();
    
    // Show voting summary
    show_voting_summary(&results);
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BackgroundDetection {
    Dark,
    Light,
    Unknown,
}

fn detect_via_colorfgbg() -> BackgroundDetection {
    if let Ok(colorfgbg) = std::env::var("COLORFGBG") {
        println!("    COLORFGBG value: '{}'", colorfgbg);
        
        if let Some(bg) = colorfgbg.split(';').nth(1) {
            if let Ok(bg_color) = bg.parse::<u8>() {
                println!("    Background color: {} ({})", bg_color, 
                    if bg_color < 8 { "dark range 0-7" } else { "bright range 8-15" });
                
                // Colors 0-7 are typically dark, 8-15 are bright
                return if bg_color < 8 { 
                    BackgroundDetection::Dark 
                } else { 
                    BackgroundDetection::Light 
                };
            } else {
                println!("    Failed to parse background color: '{}'", bg);
            }
        } else {
            println!("    No background component found");
        }
    } else {
        println!("    COLORFGBG not set");
    }
    
    BackgroundDetection::Unknown
}

fn detect_via_term_program() -> BackgroundDetection {
    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        println!("    TERM_PROGRAM: '{}'", term_program);
        
        match term_program.as_str() {
            "Apple_Terminal" => {
                println!("    macOS Terminal.app detected - commonly dark");
                BackgroundDetection::Dark
            }
            "iTerm.app" => {
                println!("    iTerm2 detected - commonly dark");
                BackgroundDetection::Dark
            }
            "vscode" => {
                println!("    VS Code integrated terminal - often dark");
                BackgroundDetection::Dark
            }
            "Hyper" => {
                println!("    Hyper terminal - commonly dark");
                BackgroundDetection::Dark
            }
            "Windows Terminal" => {
                println!("    Windows Terminal - mixed, assume dark");
                BackgroundDetection::Dark
            }
            _ => {
                println!("    Unknown terminal program");
                BackgroundDetection::Unknown
            }
        }
    } else {
        println!("    TERM_PROGRAM not set");
        BackgroundDetection::Unknown
    }
}

fn detect_via_theme_vars() -> BackgroundDetection {
    let theme_vars = ["DARK_MODE", "THEME", "BACKGROUND"];
    
    for var in &theme_vars {
        if let Ok(value) = std::env::var(var) {
            let lower = value.to_lowercase();
            println!("    {}: '{}'", var, value);
            
            if lower.contains("dark") || lower.contains("black") {
                println!("    -> Indicates dark theme");
                return BackgroundDetection::Dark;
            }
            if lower.contains("light") || lower.contains("white") {
                println!("    -> Indicates light theme");
                return BackgroundDetection::Light;
            }
        }
    }
    
    println!("    No theme variables set");
    BackgroundDetection::Unknown
}

fn detect_via_specific_indicators() -> BackgroundDetection {
    // Check for VS Code
    if std::env::var("VSCODE_INJECTION").is_ok() {
        println!("    VS Code injection detected");
        return BackgroundDetection::Dark;
    }
    
    // Check for iTerm session
    if std::env::var("ITERM_SESSION_ID").is_ok() {
        println!("    iTerm session detected");
        return BackgroundDetection::Dark;
    }
    
    // Check for Windows Terminal
    if std::env::var("WT_SESSION").is_ok() {
        println!("    Windows Terminal session detected");
        return BackgroundDetection::Dark;
    }
    
    println!("    No specific indicators found");
    BackgroundDetection::Unknown
}

fn format_result(result: BackgroundDetection) -> String {
    match result {
        BackgroundDetection::Dark => "🌙 Dark".to_string(),
        BackgroundDetection::Light => "☀️ Light".to_string(),
        BackgroundDetection::Unknown => "❓ Unknown".to_string(),
    }
}

fn show_voting_summary(results: &HashMap<&str, BackgroundDetection>) {
    println!("📊 Detection Summary:");
    
    let mut dark_votes = 0;
    let mut light_votes = 0;
    let mut unknown_votes = 0;
    
    for (strategy, result) in results {
        match result {
            BackgroundDetection::Dark => {
                dark_votes += 1;
                println!("  {} votes: 🌙 Dark", strategy);
            }
            BackgroundDetection::Light => {
                light_votes += 1;
                println!("  {} votes: ☀️ Light", strategy);
            }
            BackgroundDetection::Unknown => {
                unknown_votes += 1;
                println!("  {} votes: ❓ Unknown", strategy);
            }
        }
    }
    
    println!();
    println!("  Vote Tally:");
    println!("    🌙 Dark:    {}", dark_votes);
    println!("    ☀️ Light:   {}", light_votes);
    println!("    ❓ Unknown: {}", unknown_votes);
    
    println!();
}

fn show_final_recommendation() {
    let final_detection = detect_background_comprehensive();
    
    println!("🎯 Final Recommendation:");
    match final_detection {
        BackgroundDetection::Dark => {
            println!("  Background: 🌙 DARK");
            println!("  Theme:      Dark theme (exact iidy-js compatibility)");
            println!("  Colors:     xterm(255) white, xterm(253) gray, magenta primary");
        }
        BackgroundDetection::Light => {
            println!("  Background: ☀️ LIGHT");
            println!("  Theme:      Light theme (adapted for visibility)");
            println!("  Colors:     Black text, xterm(240) gray, xterm(90) purple primary");
        }
        BackgroundDetection::Unknown => {
            println!("  Background: ❓ UNKNOWN");
            println!("  Theme:      Default to Dark (iidy-js compatibility)");
            println!("  Colors:     Same as dark theme");
        }
    }
    
    println!();
    println!("💡 Testing Recommendations:");
    println!("  - Try: COLORFGBG=15;0 (white fg, black bg)");
    println!("  - Try: COLORFGBG=0;15 (black fg, white bg)");
    println!("  - Try: THEME=dark or THEME=light");
    println!("  - Try: --theme=dark or --theme=light flags");
    
    println!();
    show_theme_comparison();
}

/// Comprehensive detection using all strategies
fn detect_background_comprehensive() -> BackgroundDetection {
    // Strategy 1: COLORFGBG (most reliable when available)
    let colorfgbg_result = detect_via_colorfgbg();
    if colorfgbg_result != BackgroundDetection::Unknown {
        return colorfgbg_result;
    }
    
    // Strategy 2: Explicit theme variables
    let theme_result = detect_via_theme_vars();
    if theme_result != BackgroundDetection::Unknown {
        return theme_result;
    }
    
    // Strategy 3: Terminal program heuristics
    let term_result = detect_via_term_program();
    if term_result != BackgroundDetection::Unknown {
        return term_result;
    }
    
    // Strategy 4: Specific indicators
    let indicators_result = detect_via_specific_indicators();
    if indicators_result != BackgroundDetection::Unknown {
        return indicators_result;
    }
    
    // Default: assume dark (iidy-js compatibility)
    BackgroundDetection::Dark
}

fn show_theme_comparison() {
    println!("🎨 Theme Color Comparison:");
    
    use owo_colors::OwoColorize;
    
    if std::io::stdout().is_terminal() && std::env::var("NO_COLOR").is_err() {
        println!();
        println!("Dark Theme Colors (exact iidy-js):");
        println!("  Section Heading:  {}", "Stack Details:".bold());
        println!("  Primary Value:    {}", "my-stack-name".magenta());
        println!("  Secondary Value:  {}", "us-east-1".truecolor(128, 128, 128));
        println!("  Status Complete:  {}", "CREATE_COMPLETE".green());
        println!("  Status Failed:    {}", "CREATE_FAILED".bright_red());
        println!("  Status Progress:  {}", "CREATE_IN_PROGRESS".yellow());
        
        println!();
        println!("Light Theme Colors (adapted):");
        println!("  Section Heading:  {}", "Stack Details:".black().bold());
        println!("  Primary Value:    {}", "my-stack-name".purple()); // Dark purple approximation
        println!("  Secondary Value:  {}", "us-east-1".bright_black()); // Medium gray approximation
        println!("  Status Complete:  {}", "CREATE_COMPLETE".green()); // Dark green
        println!("  Status Failed:    {}", "CREATE_FAILED".red());
        println!("  Status Progress:  {}", "CREATE_IN_PROGRESS".yellow().dimmed()); // Dark orange approximation
        
        println!();
        println!("High Contrast Theme:");
        println!("  Section Heading:  {}", "Stack Details:".bright_white().bold());
        println!("  Primary Value:    {}", "my-stack-name".bright_magenta());
        println!("  Secondary Value:  {}", "us-east-1".white());
        println!("  Status Complete:  {}", "CREATE_COMPLETE".bright_green());
        println!("  Status Failed:    {}", "CREATE_FAILED".bright_red());
        println!("  Status Progress:  {}", "CREATE_IN_PROGRESS".bright_yellow());
    } else {
        println!("  [Colors disabled - run in TTY with color support to see theme comparison]");
    }
    
    println!();
}

fn demonstrate_theme_override(explicit_theme: Option<Theme>) {
    println!("🎨 Theme Override Demonstration:");
    
    if let Some(cli_theme) = explicit_theme {
        println!("  🎯 Explicit theme specified: {:?}", cli_theme);
        println!("  ✅ Theme override takes priority over auto-detection");
        println!();
        
        // Convert CLI theme to terminal theme
        let terminal_theme = convert_cli_theme_to_terminal(cli_theme);
        demonstrate_cloudformation_output(terminal_theme);
        show_cli_examples();
    } else {
        println!("  🔍 No explicit theme specified - using auto-detection");
        let detected = detect_background_comprehensive();
        let terminal_theme = match detected {
            BackgroundDetection::Dark => TerminalTheme::Dark,
            BackgroundDetection::Light => TerminalTheme::Light,
            BackgroundDetection::Unknown => TerminalTheme::Dark, // Default to dark for iidy-js compatibility
        };
        
        println!("  🎯 Auto-detected theme: {:?}", terminal_theme);
        println!();
        
        demonstrate_cloudformation_output(terminal_theme);
        demonstrate_all_themes();
        show_cli_examples();
    }
}

/// Convert CLI theme enum to terminal theme enum
fn convert_cli_theme_to_terminal(cli_theme: Theme) -> TerminalTheme {
    match cli_theme {
        Theme::Auto => TerminalTheme::Auto,
        Theme::Light => TerminalTheme::Light,
        Theme::Dark => TerminalTheme::Dark,
        Theme::HighContrast => TerminalTheme::HighContrast,
    }
}

fn demonstrate_cloudformation_output(theme: TerminalTheme) {
    use owo_colors::OwoColorize;
    
    println!("📋 CloudFormation Output Example (using {:?} theme):", theme);
    
    let caps = TerminalCapabilities::detect();
    let color_theme = ColorTheme::for_theme(theme, &caps);
    
    if std::io::stdout().is_terminal() && std::env::var("NO_COLOR").is_err() {
        println!();
        println!("Stack Details:");
        println!("  Name:          {}", "my-production-stack".color(color_theme.resource_id));
        println!("  Region:        {}", "us-east-1".color(color_theme.muted));
        println!("  Status:        {}", "CREATE_COMPLETE".color(color_theme.success));
        println!("  Created:       {}", "2025-06-17T12:34:56Z".color(color_theme.timestamp));
        println!();
        
        println!("Recent Events:");
        println!("  {} {} {}", 
            "12:34:56".color(color_theme.timestamp),
            "MyBucket".color(color_theme.resource_id), 
            "CREATE_COMPLETE".color(color_theme.success)
        );
        println!("  {} {} {}", 
            "12:34:45".color(color_theme.timestamp),
            "MyBucket".color(color_theme.resource_id), 
            "CREATE_IN_PROGRESS".color(color_theme.in_progress)
        );
        println!("  {} {} {}", 
            "12:34:30".color(color_theme.timestamp),
            "MySecurityGroup".color(color_theme.resource_id), 
            "CREATE_COMPLETE".color(color_theme.success)
        );
    } else {
        println!("  [Colors disabled - enable TTY and remove NO_COLOR to see themed output]");
    }
    
    println!();
}

fn demonstrate_all_themes() {
    println!("🌈 All Available Themes:");
    
    let themes = vec![
        (TerminalTheme::Dark, "Dark (iidy-js compatible, optimized for dark terminals)"),
        (TerminalTheme::Light, "Light (optimized for light terminals)"),
        (TerminalTheme::HighContrast, "High Contrast (accessibility focused)"),
        (TerminalTheme::Auto, "Auto (detects terminal background)"),
    ];
    
    for (theme, description) in themes {
        println!();
        println!("  {:?}: {}", theme, description);
        
        if std::io::stdout().is_terminal() && std::env::var("NO_COLOR").is_err() {
            let caps = TerminalCapabilities::detect();
            let color_theme = ColorTheme::for_theme(theme, &caps);
            
            use owo_colors::OwoColorize;
            print!("    Sample: ");
            print!("{} ", "SUCCESS".color(color_theme.success));
            print!("{} ", "ERROR".color(color_theme.error));
            print!("{} ", "WARNING".color(color_theme.warning));
            print!("{}", "INFO".color(color_theme.info));
            println!();
        }
    }
    
    println!();
}

fn show_cli_examples() {
    println!("💡 CLI Usage Examples:");
    println!();
    println!("  # Use auto-detection (default):");
    println!("  cargo run --bin iidy-pocs detect-background");
    println!();
    println!("  # Force specific themes:");
    println!("  cargo run --bin iidy-pocs detect-background --theme=dark");
    println!("  cargo run --bin iidy-pocs detect-background --theme=light");
    println!("  cargo run --bin iidy-pocs detect-background --theme=high-contrast");
    println!("  cargo run --bin iidy-pocs detect-background --theme=auto");
    println!();
    println!("  # Test with different environment variables:");
    println!("  COLORFGBG=15;0 cargo run --bin iidy-pocs detect-background    # Dark bg");
    println!("  COLORFGBG=0;15 cargo run --bin iidy-pocs detect-background    # Light bg");
    println!("  THEME=dark cargo run --bin iidy-pocs detect-background         # Explicit dark");
    println!("  THEME=light cargo run --bin iidy-pocs detect-background        # Explicit light");
    println!();
    println!("  # Override detection with explicit theme:");
    println!("  COLORFGBG=0;15 cargo run --bin iidy-pocs detect-background --theme=dark");
    println!("  # (Will detect light bg but use dark theme due to --theme flag)");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    
    #[test]
    fn test_colorfgbg_detection() {
        // Test dark background
        unsafe { env::set_var("COLORFGBG", "15;0") };
        assert_eq!(detect_via_colorfgbg(), BackgroundDetection::Dark);
        
        // Test light background  
        unsafe { env::set_var("COLORFGBG", "0;15") };
        assert_eq!(detect_via_colorfgbg(), BackgroundDetection::Light);
        
        // Test invalid format
        unsafe { env::set_var("COLORFGBG", "invalid") };
        assert_eq!(detect_via_colorfgbg(), BackgroundDetection::Unknown);
        
        // Clean up
        unsafe { env::remove_var("COLORFGBG") };
    }
    
    #[test]
    fn test_term_program_detection() {
        // Test known terminals
        unsafe { env::set_var("TERM_PROGRAM", "Apple_Terminal") };
        assert_eq!(detect_via_term_program(), BackgroundDetection::Dark);
        
        unsafe { env::set_var("TERM_PROGRAM", "iTerm.app") };
        assert_eq!(detect_via_term_program(), BackgroundDetection::Dark);
        
        unsafe { env::set_var("TERM_PROGRAM", "unknown") };
        assert_eq!(detect_via_term_program(), BackgroundDetection::Unknown);
        
        // Clean up
        unsafe { env::remove_var("TERM_PROGRAM") };
    }
    
    #[test]
    fn test_theme_vars_detection() {
        // Test dark indicators
        unsafe { env::set_var("THEME", "dark") };
        assert_eq!(detect_via_theme_vars(), BackgroundDetection::Dark);
        
        // Test light indicators
        unsafe { env::set_var("THEME", "light") };
        assert_eq!(detect_via_theme_vars(), BackgroundDetection::Light);
        
        // Clean up
        unsafe { env::remove_var("THEME") };
    }
}