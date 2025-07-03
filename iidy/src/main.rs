use clap::{CommandFactory, Parser, error::ErrorKind};
use clap_complete::{Shell, generate};
use env_logger;
use log::debug;
use std::io;

use iidy::{
    cfn,
    cli::{Cli, Commands},
    color::ColorContext,
    explain::handle_explain_command,
    render::handle_render_command,
};
mod demo;
use tokio::runtime::Runtime;

fn handle_command(cli: Cli) {
    let rt = Runtime::new().expect("failed to create tokio runtime");
    match cli.command {
        Commands::CreateStack(ref args) => {
            match rt.block_on(cfn::create_stack::create_stack(&cli, &args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error creating stack: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::UpdateStack(ref args) => {
            match rt.block_on(cfn::update_stack::update_stack(&cli, &args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error updating stack: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::CreateOrUpdate(ref args) => {
            match rt.block_on(cfn::create_or_update::create_or_update(&cli, &args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error creating or updating stack: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::EstimateCost(ref args) => {
            if let Err(e) = rt.block_on(cfn::estimate_cost::estimate_cost(&cli, &args)) {
                eprintln!("error estimating cost: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::DummySpacer => {}
        Commands::CreateChangeset(ref args) => {
            if let Err(e) = rt.block_on(cfn::create_changeset::create_changeset(&cli, &args)) {
                eprintln!("error creating change set: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::ExecChangeset(ref args) => {
            if let Err(e) = rt.block_on(cfn::exec_changeset::exec_changeset(&cli, &args)) {
                eprintln!("error executing change set: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::DummySpacer2 => {}
        Commands::DescribeStack(ref args) => {
            if let Err(e) =
                rt.block_on(cfn::describe_stack::describe_stack(&cli, &args))
            {
                eprintln!("error describing stack: {e:?}");
                std::process::exit(1);
            }
        }

        Commands::DescribeStackDrift(ref args) => {
            if let Err(e) = rt.block_on(cfn::describe_stack_drift::describe_stack_drift(&cli, &args)) {
                eprintln!("error describing stack drift: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::WatchStack(ref args) => {
            if let Err(e) = rt.block_on(cfn::watch_stack::watch_stack(&cli, &args)) {
                eprintln!("error watching stack: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::DeleteStack(ref args) => {
            match rt.block_on(cfn::delete_stack::delete_stack(&cli, &args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error deleting stack: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::GetStackTemplate(ref args) => {
            match rt.block_on(cfn::get_stack_template::get_stack_template(&cli, &args)) {
                Ok(out) => {
                    for line in out.stderr_lines {
                        eprintln!("{line}");
                    }
                    println!("{}", out.body);
                }
                Err(e) => {
                    eprintln!("error getting template: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::GetStackInstances(ref args) => {
            if let Err(e) = rt.block_on(cfn::get_stack_instances::get_stack_instances(&cli, &args)) {
                eprintln!("error getting stack instances: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::ListStacks(ref args) => {
            if let Err(e) = rt.block_on(cfn::list_stacks::list_stacks(&cli, &args)) {
                eprintln!("error listing stacks: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::DummySpacer3 => {}
        Commands::Param { command } => println!("param {:?}", command),
        Commands::DummySpacer4 => {}
        Commands::TemplateApproval { command } => println!("template-approval {:?}", command),
        Commands::DummySpacer5 => {}
        Commands::Render(args) => {
            if let Err(e) = rt.block_on(handle_render_command(&args)) {
                eprintln!(); // Add blank line before errors for better readability
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        Commands::GetImport(args) => println!("get-import {:?}", args),
        Commands::Demo(args) => {
            if let Err(e) = rt.block_on(demo::run(&args.demoscript, args.timescaling)) {
                eprintln!("demo failed: {e:?}");
                std::process::exit(1);
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
        Commands::Explain { codes } => {
            handle_explain_command(codes);
        }
    }
}

fn main() {
    env_logger::Builder::from_default_env().init();

    match Cli::try_parse() {
        Ok(cli) => {
            debug!("CLI options: {:?}", cli);

            // TODO: see if we can get rid of this global color setup.
            // I think it was introduced when implementing yaml error handling

            // Initialize color context early for global access
            let theme = match cli.global_opts.theme {
                iidy::cli::Theme::Auto => iidy::terminal::Theme::Auto,
                iidy::cli::Theme::Light => iidy::terminal::Theme::Light,
                iidy::cli::Theme::Dark => iidy::terminal::Theme::Dark,
                iidy::cli::Theme::HighContrast => iidy::terminal::Theme::HighContrast,
            };
            ColorContext::init_global(cli.global_opts.color.clone(), theme);

            handle_command(cli)
        }
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
