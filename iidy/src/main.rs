mod cli;
use clap::Parser;
use cli::{Cli, Commands};

fn main() {
    let cli = Cli::parse();
    println!("CLI options: {:?}", cli);
    match cli.command {
        Commands::CreateStack(args) => println!("create-stack {:?}", args),
        Commands::UpdateStack(args) => println!("update-stack {:?}", args),
        Commands::CreateOrUpdate(args) => println!("create-or-update {:?}", args),
        Commands::EstimateCost(args) => println!("estimate-cost {:?}", args),
        Commands::DummySpacer => {},
        Commands::CreateChangeset(args) => println!("create-changeset {:?}", args),
        Commands::ExecChangeset(args) => println!("exec-changeset {:?}", args),
        Commands::DummySpacer2 => {},
        Commands::DescribeStack(args) => println!("describe-stack {:?}", args),
        Commands::WatchStack(args) => println!("watch-stack {:?}", args),
        Commands::DescribeStackDrift(args) => println!("describe-stack-drift {:?}", args),
        Commands::DeleteStack(args) => println!("delete-stack {:?}", args),
        Commands::GetStackTemplate(args) => println!("get-stack-template {:?}", args),
        Commands::GetStackInstances(args) => println!("get-stack-instances {:?}", args),
        Commands::ListStacks(args) => println!("list-stacks {:?}", args),
        Commands::DummySpacer3 => {},
        Commands::Param { command } => println!("param {:?}", command),
        Commands::DummySpacer4 => {},
        Commands::TemplateApproval { command } => println!("template-approval {:?}", command),
        Commands::DummySpacer5 => {},
        Commands::Render(args) => println!("render {:?}", args),
        Commands::GetImport(args) => println!("get-import {:?}", args),
        Commands::Demo(args) => println!("demo {:?}", args),
        Commands::LintTemplate(args) => println!("lint-template {:?}", args),
        Commands::ConvertStackToIidy(args) => println!("convert-stack-to-iidy {:?}", args),
        Commands::InitStackArgs(args) => println!("init-stack-args {:?}", args),
        Commands::DummySpacer6 => {},
    }
}
