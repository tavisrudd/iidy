use anyhow::Result;
use clap::{Parser, Subcommand};
use iidy::pocs::{ratatui_demo};

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
    /// Demonstrate ratatui TUI for describe-stack
    RatatuiDemo,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::RatatuiDemo => {
            if let Err(e) = ratatui_demo::run_ratatui_demo() {
                eprintln!("Error running ratatui demo: {}", e);
            }
        }
    }

    Ok(())
}
