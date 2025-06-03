use std::io;
use log::debug;
use env_logger;
mod aws;
mod cli;
mod display;
mod preprocess;
mod stack_args;
mod yaml;
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
    pub mod is_terminal_status;
    pub mod list_stacks;
    pub mod update_stack;
    pub mod watch_stack;
}
use clap::{CommandFactory, Parser, error::ErrorKind};
use clap_complete::{Shell, generate};
mod demo;
use cli::{Cli, Commands};
use tokio::runtime::Runtime;

fn handle_command(cli: Cli) {
    let rt = Runtime::new().expect("failed to create tokio runtime");
    match cli.command {
        Commands::CreateStack(args) => {
            if let Err(e) = rt.block_on(cfn::create_stack::create_stack(&cli.aws_opts, &args)) {
                eprintln!("error creating stack: {e:?}");
            }
        }
        Commands::UpdateStack(args) => {
            if let Err(e) = rt.block_on(cfn::update_stack::update_stack(&cli.aws_opts, &args)) {
                eprintln!("error updating stack: {e:?}");
            }
        }
        Commands::CreateOrUpdate(args) => {
            if let Err(e) = rt.block_on(cfn::create_or_update::create_or_update(
                &cli.aws_opts,
                &args,
            )) {
                eprintln!("error creating or updating stack: {e:?}");
            }
        }
        Commands::EstimateCost(args) => {
            if let Err(e) = rt.block_on(cfn::estimate_cost::estimate_cost(&cli.aws_opts, &args)) {
                eprintln!("error estimating cost: {e:?}");
            }
        }
        Commands::DummySpacer => {}
        Commands::CreateChangeset(args) => {
            if let Err(e) = rt.block_on(cfn::create_changeset::create_changeset(
                &cli.aws_opts,
                &args,
            )) {
                eprintln!("error creating change set: {e:?}");
            }
        }
        Commands::ExecChangeset(args) => {
            if let Err(e) = rt.block_on(cfn::exec_changeset::exec_changeset(&cli.aws_opts, &args)) {
                eprintln!("error executing change set: {e:?}");
            }
        }
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
        Commands::DeleteStack(args) => {
            if let Err(e) = rt.block_on(cfn::delete_stack::delete_stack(&cli.aws_opts, &args)) {
                eprintln!("error deleting stack: {e:?}");
            }
        }
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
        Commands::GetStackInstances(args) => {
            if let Err(e) = rt.block_on(cfn::get_stack_instances::get_stack_instances(
                &cli.aws_opts,
                &args,
            )) {
                eprintln!("error getting stack instances: {e:?}");
            }
        }
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
            debug!("Completion for {:?}", shell);
        }
    }
}

fn main() {
    env_logger::Builder::from_default_env().init();

    match Cli::try_parse() {
        Ok(cli) => {
            debug!("CLI options: {:?}", cli);
            handle_command(cli)
        },
        Err(e)
            if matches!(
                e.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) =>
        {
            e.print().unwrap();
            std::process::exit(0); // 👈 override exit code 2 → 0
        }
        Err(e) => {
            e.print().unwrap();
            std::process::exit(1); // non-help errors still exit with error
        }
    }
}
