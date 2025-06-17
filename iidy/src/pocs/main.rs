use anyhow::Result;
use clap::{Parser, Subcommand};
use iidy::cli::{ColorChoice, Theme};
use iidy::color::ColorContext;
use iidy::pocs::{detect_background, ratatui_demo, spinner_demo, theme_demo};

#[derive(Parser)]
#[command(
    name = "iidy-pocs",
    about = "Proof of concepts and demonstrations for iidy features",
    version,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Detect terminal background and validate theme selection
    DetectBackground {
        /// Override theme selection (takes priority over auto-detection)
        #[arg(long, value_enum)]
        theme: Option<Theme>,
    },
    /// Demonstrate color themes and terminal capabilities
    ThemeDemo,
    /// Demonstrate spinner and progress indicators
    SpinnerDemo,
    /// Demonstrate ratatui TUI for describe-stack
    RatatuiDemo,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize color context for demos with default theme
    // (individual commands can override this)
    ColorContext::init_global(ColorChoice::Auto, iidy::terminal::Theme::Auto);

    match cli.command {
        Commands::DetectBackground { theme } => {
            detect_background::run_detect_background_demo(theme);
        }
        Commands::ThemeDemo => {
            theme_demo::run_theme_demo();
        }
        Commands::SpinnerDemo => {
            spinner_demo::run_spinner_demo();
        }
        Commands::RatatuiDemo => {
            if let Err(e) = ratatui_demo::run_ratatui_demo() {
                eprintln!("Error running ratatui demo: {}", e);
            }
        }
    }

    Ok(())
}
