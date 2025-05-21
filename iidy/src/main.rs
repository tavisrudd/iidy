use clap::{Parser, Subcommand, Args};

#[derive(Parser, Debug)]
#[command(name = "iidy", about = "Rust port of Unbounce/iidy", version)]
pub struct Cli {
    /// AWS credentials profile
    #[arg(long, global = true)]
    profile: Option<String>,

    /// AWS region
    #[arg(long, global = true)]
    region: Option<String>,

    /// IAM role to assume
    #[arg(long, value_name = "ARN", global = true)]
    role_arn: Option<String>,

    /// Run without prompting for confirmation
    #[arg(long, global = true)]
    non_interactive: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Render a CloudFormation template with parameters
    Render(RenderArgs),
    /// Validate a CloudFormation template
    Validate(TemplateArgs),
    /// Estimate the cost of a template
    EstimateCost(TemplateArgs),
    /// Create a new CloudFormation stack
    CreateStack(StackArgs),
    /// Update an existing CloudFormation stack
    UpdateStack(StackArgs),
    /// Delete a CloudFormation stack
    DeleteStack(StackNameArgs),
    /// Show the diff between a template and the deployed stack
    DiffStacks(StackArgs),
    /// Dump evaluated parameters
    DumpParams(TemplateArgs),
    /// Dump the rendered template
    DumpTemplate(TemplateArgs),
}

#[derive(Args, Debug)]
struct TemplateArgs {
    /// Path to the CloudFormation template
    template: String,
    /// Parameters file
    #[arg(short, long, value_name = "FILE")]
    params: Option<String>,
}

#[derive(Args, Debug)]
struct RenderArgs {
    #[command(flatten)]
    template: TemplateArgs,
    /// Output file (defaults to stdout)
    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,
}

#[derive(Args, Debug)]
struct StackNameArgs {
    /// Name of the CloudFormation stack
    stack_name: String,
}

#[derive(Args, Debug)]
struct StackArgs {
    /// Name of the CloudFormation stack
    stack_name: String,
    /// Path to the CloudFormation template
    template: String,
    /// Parameters file
    #[arg(short, long, value_name = "FILE")]
    params: Option<String>,
}

fn main() {
    let cli = Cli::parse();
    println!("CLI options: {:?}", cli);
    match cli.command {
        Commands::Render(args) => println!("Render {:?}", args),
        Commands::Validate(args) => println!("Validate {:?}", args),
        Commands::EstimateCost(args) => println!("Estimate cost {:?}", args),
        Commands::CreateStack(args) => println!("Create stack {:?}", args),
        Commands::UpdateStack(args) => println!("Update stack {:?}", args),
        Commands::DeleteStack(args) => println!("Delete stack {:?}", args),
        Commands::DiffStacks(args) => println!("Diff stacks {:?}", args),
        Commands::DumpParams(args) => println!("Dump params {:?}", args),
        Commands::DumpTemplate(args) => println!("Dump template {:?}", args),
    }
}

