use std::io;
mod aws;
mod cli;
mod describe_stack;
mod list_stacks;
use clap::{CommandFactory, Parser};
use clap_complete::{Shell, generate};
use cli::{Cli, Commands};
use tokio::runtime::Runtime;

fn main() {
    let cli = Cli::parse();
    println!("CLI options: {:?}", cli);
    let rt = Runtime::new().expect("failed to create tokio runtime");
    match cli.command {
        Commands::CreateStack(args) => println!("create-stack {:?}", args),
        Commands::UpdateStack(args) => println!("update-stack {:?}", args),
        Commands::CreateOrUpdate(args) => println!("create-or-update {:?}", args),
        Commands::EstimateCost(args) => println!("estimate-cost {:?}", args),
        Commands::DummySpacer => {}
        Commands::CreateChangeset(args) => println!("create-changeset {:?}", args),
        Commands::ExecChangeset(args) => println!("exec-changeset {:?}", args),
        Commands::DummySpacer2 => {}
        Commands::DescribeStack(args) => {
            match rt.block_on(describe_stack::describe_stack(&cli.aws_opts, &args)) {
                Ok(lines) => {
                    for line in lines {
                        println!("{line}");
                    }
                }
                Err(e) => eprintln!("error describing stack: {e:?}"),
            }
        }
        Commands::WatchStack(args) => println!("watch-stack {:?}", args),
        Commands::DescribeStackDrift(args) => println!("describe-stack-drift {:?}", args),
        Commands::DeleteStack(args) => println!("delete-stack {:?}", args),
        Commands::GetStackTemplate(args) => println!("get-stack-template {:?}", args),
        Commands::GetStackInstances(args) => println!("get-stack-instances {:?}", args),
        Commands::ListStacks(args) => {
            match rt.block_on(list_stacks::list_stacks(&cli.aws_opts, &args)) {
                Ok(lines) => {
                    for line in lines {
                        println!("{line}");
                    }
                }
                Err(e) => eprintln!("error listing stacks: {e:?}"),
            }
        }
        Commands::DummySpacer3 => {}
        Commands::Param { command } => println!("param {:?}", command),
        Commands::DummySpacer4 => {}
        Commands::TemplateApproval { command } => println!("template-approval {:?}", command),
        Commands::DummySpacer5 => {}
        Commands::Render(args) => println!("render {:?}", args),
        Commands::GetImport(args) => println!("get-import {:?}", args),
        Commands::Demo(args) => println!("demo {:?}", args),
        Commands::LintTemplate(args) => println!("lint-template {:?}", args),
        Commands::ConvertStackToIidy(args) => println!("convert-stack-to-iidy {:?}", args),
        Commands::InitStackArgs(args) => println!("init-stack-args {:?}", args),
        Commands::DummySpacer6 => {}
        Commands::Completion { shell } => {
            let shell = shell
                .or(Shell::from_env())
                .expect("invalid shell argument or $SHELL env var");
            generate(shell, &mut Cli::command(), "iidy-rs", &mut io::stdout());
        }
    }
}
