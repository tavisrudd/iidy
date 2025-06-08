use clap::{Parser, Subcommand};
use anyhow::Result;
use iidy::color::ColorContext;
use iidy::terminal::Theme;
use iidy::cli::ColorChoice;
use iidy::pocs::{theme_demo, spinner_demo, ratatui_demo, custom_serializer_demo};

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
    /// Demonstrate color themes and terminal capabilities
    ThemeDemo,
    /// Demonstrate spinner and progress indicators
    SpinnerDemo,
    /// Demonstrate ratatui TUI for describe-stack
    RatatuiDemo,
    /// Demonstrate custom YAML serializer for CloudFormation intrinsics
    CustomSerializerDemo,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize color context for demos
    ColorContext::init_global(ColorChoice::Auto, Theme::Auto);

    match cli.command {
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
        Commands::CustomSerializerDemo => {
            // Use tokio runtime for async demo
            let rt = tokio::runtime::Runtime::new()?;
            if let Err(e) = rt.block_on(custom_serializer_demo::run_custom_serializer_demo()) {
                eprintln!("Error running custom serializer demo: {}", e);
            }
        }
    }

    Ok(())
}