use std::io;
mod aws;
mod cli;
mod display;
mod cfn {
    pub mod create_changeset;
    pub mod create_or_update;
    pub mod create_stack;
    pub mod delete_stack;
    pub mod describe_stack;
    pub mod describe_stack_drift;
    pub mod estimate_cost;
    pub mod exec_changeset;
    pub mod get_stack_instances;
    pub mod get_stack_template;
    pub mod list_stacks;
    pub mod update_stack;
    pub mod is_terminal_status;
    pub mod watch_stack;
}
use clap::{CommandFactory, Parser};
use clap_complete::{Shell, generate};
mod demo;
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
            if let Err(e) = rt.block_on(cfn::describe_stack::describe_stack(&cli.aws_opts, &args)) {
                eprintln!("error describing stack: {e:?}");
            }
        }

        Commands::DescribeStackDrift(args) => {
            if let Err(e) = rt.block_on(cfn::describe_stack_drift::describe_stack_drift(
                &cli.aws_opts,
                &args,
            )) {
                eprintln!("error describing stack drift: {e:?}");
            }
        }
        Commands::WatchStack(args) => {
            if let Err(e) = rt.block_on(cfn::watch_stack::watch_stack(&cli.aws_opts, &args)) {
                eprintln!("error watching stack: {e:?}");
            }
        }
        Commands::DeleteStack(args) => println!("delete-stack {:?}", args),
        Commands::GetStackTemplate(args) => {
            match rt.block_on(cfn::get_stack_template::get_stack_template(
                &cli.aws_opts,
                &args,
            )) {
                Ok(out) => {
                    for line in out.stderr_lines {
                        eprintln!("{line}");
                    }
                    println!("{}", out.body);
                }
                Err(e) => eprintln!("error getting template: {e:?}"),
            }
        }
        Commands::GetStackInstances(args) => println!("get-stack-instances {:?}", args),
        Commands::ListStacks(args) => {
            if let Err(e) = rt.block_on(cfn::list_stacks::list_stacks(&cli.aws_opts, &args)) {
                eprintln!("error listing stacks: {e:?}");
            }
        }
        Commands::DummySpacer3 => {}
        Commands::Param { command } => println!("param {:?}", command),
        Commands::DummySpacer4 => {}
        Commands::TemplateApproval { command } => println!("template-approval {:?}", command),
        Commands::DummySpacer5 => {}
        Commands::Render(args) => println!("render {:?}", args),
        Commands::GetImport(args) => println!("get-import {:?}", args),
        Commands::Demo(args) => {
            if let Err(e) = rt.block_on(demo::run(&args.demoscript, args.timescaling)) {
                eprintln!("demo failed: {e:?}");
            }
        }
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
