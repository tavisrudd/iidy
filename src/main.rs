use clap::{CommandFactory, Parser, error::ErrorKind};
use clap_complete::{Shell, generate};
use log::debug;
use std::io;

use iidy::{
    cfn,
    cli::{ApprovalCommands, Cli, Commands, ParamCommands},
    explain::handle_explain_command,
    output::color::ColorContext,
    output::terminal::Theme as TerminalTheme,
    params,
    render::handle_render_command,
};
mod demo;
use tokio::runtime::Runtime;

fn handle_command(cli: Cli) {
    // Set AWS_SDK_LOAD_CONFIG before creating the Tokio runtime so no other threads exist.
    // This must not be done inside async code where Tokio worker threads are alive.
    if let Some(home) = std::env::var_os("HOME") {
        let aws_dir = std::path::Path::new(&home).join(".aws");
        if aws_dir.exists() {
            unsafe {
                std::env::set_var("AWS_SDK_LOAD_CONFIG", "1");
            }
        }
    }

    let rt = Runtime::new().expect("failed to create tokio runtime");
    match cli.command {
        Commands::CreateStack(ref args) => {
            match rt.block_on(cfn::create_stack::create_stack(&cli, args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error creating stack: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::UpdateStack(ref args) => {
            match rt.block_on(cfn::update_stack::update_stack(&cli, args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error updating stack: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::CreateOrUpdate(ref args) => {
            match rt.block_on(cfn::create_or_update::create_or_update(&cli, args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error creating or updating stack: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::EstimateCost(ref args) => {
            match rt.block_on(cfn::estimate_cost::estimate_cost(&cli, args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error estimating cost: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::DummySpacer => {}
        Commands::CreateChangeset(ref args) => {
            if let Err(e) = rt.block_on(cfn::create_changeset::create_changeset(&cli, args)) {
                eprintln!("error creating change set: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::ExecChangeset(ref args) => {
            match rt.block_on(cfn::exec_changeset::exec_changeset(&cli, args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error executing change set: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::DummySpacer2 => {}
        Commands::DescribeStack(ref args) => {
            match rt.block_on(cfn::describe_stack::describe_stack(&cli, args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error describing stack: {e:?}");
                    std::process::exit(1);
                }
            }
        }

        Commands::DescribeStackDrift(ref args) => {
            if let Err(e) = rt.block_on(cfn::describe_stack_drift::describe_stack_drift(&cli, args))
            {
                eprintln!("error describing stack drift: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::WatchStack(ref args) => {
            if let Err(e) = rt.block_on(cfn::watch_stack::watch_stack(&cli, args)) {
                eprintln!("error watching stack: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::DeleteStack(ref args) => {
            match rt.block_on(cfn::delete_stack::delete_stack(&cli, args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error deleting stack: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::GetStackTemplate(ref args) => {
            match rt.block_on(cfn::get_stack_template::get_stack_template(&cli, args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error getting template: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::GetStackInstances(ref args) => {
            cfn::get_stack_instances::get_stack_instances(args);
        }
        Commands::ListStacks(ref args) => {
            if let Err(e) = rt.block_on(cfn::list_stacks::list_stacks(&cli, args)) {
                eprintln!("error listing stacks: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::DummySpacer3 => {}
        Commands::Param { ref command } => match command {
            ParamCommands::Set(args) => match rt.block_on(params::set::set_param(&cli, args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error setting parameter: {e:?}");
                    std::process::exit(1);
                }
            },
            ParamCommands::Review(args) => {
                match rt.block_on(params::review::review_param(&cli, args)) {
                    Ok(exit_code) => std::process::exit(exit_code),
                    Err(e) => {
                        eprintln!("error reviewing parameter: {e:?}");
                        std::process::exit(1);
                    }
                }
            }
            ParamCommands::Get(args) => match rt.block_on(params::get::get_param(&cli, args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error getting parameter: {e:?}");
                    std::process::exit(1);
                }
            },
            ParamCommands::GetByPath(args) => {
                match rt.block_on(params::get_by_path::get_by_path(&cli, args)) {
                    Ok(exit_code) => std::process::exit(exit_code),
                    Err(e) => {
                        eprintln!("error getting parameters by path: {e:?}");
                        std::process::exit(1);
                    }
                }
            }
            ParamCommands::GetHistory(args) => {
                match rt.block_on(params::get_history::get_history(&cli, args)) {
                    Ok(exit_code) => std::process::exit(exit_code),
                    Err(e) => {
                        eprintln!("error getting parameter history: {e:?}");
                        std::process::exit(1);
                    }
                }
            }
        },
        Commands::DummySpacer4 => {}
        Commands::TemplateApproval { ref command } => match command {
            ApprovalCommands::Request(args) => {
                match rt.block_on(cfn::template_approval_request::template_approval_request(
                    &cli, args,
                )) {
                    Ok(exit_code) => std::process::exit(exit_code),
                    Err(e) => {
                        eprintln!("error requesting template approval: {e:?}");
                        std::process::exit(1);
                    }
                }
            }
            ApprovalCommands::Review(args) => {
                match rt.block_on(cfn::template_approval_review::template_approval_review(
                    &cli, args,
                )) {
                    Ok(exit_code) => std::process::exit(exit_code),
                    Err(e) => {
                        eprintln!("error reviewing template approval: {e:?}");
                        std::process::exit(1);
                    }
                }
            }
        },
        Commands::DummySpacer5 => {}
        Commands::Render(args) => {
            if let Err(e) = rt.block_on(handle_render_command(&args)) {
                eprintln!(); // Add blank line before errors for better readability
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        Commands::GetImport(ref args) => {
            match rt.block_on(cfn::get_import::get_import(&cli, args)) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error getting import: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Demo(args) => {
            if let Err(e) = rt.block_on(demo::run(
                &args.demoscript,
                args.timescaling,
                args.mask_secrets,
            )) {
                eprintln!("demo failed: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::LintTemplate(args) => println!("lint-template {args:?}"),
        Commands::ConvertStackToIidy(ref args) => {
            match rt.block_on(cfn::convert_stack_to_iidy::convert_stack_to_iidy(
                &cli, args,
            )) {
                Ok(exit_code) => std::process::exit(exit_code),
                Err(e) => {
                    eprintln!("error converting stack to iidy: {e:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::InitStackArgs(args) => {
            if let Err(e) = cfn::init_stack_args::init_stack_args(&args) {
                eprintln!("error initializing stack args: {e:?}");
                std::process::exit(1);
            }
        }
        Commands::DummySpacer6 => {}
        Commands::Completion { shell } => {
            let shell = shell
                .or(Shell::from_env())
                .expect("invalid shell argument or $SHELL env var");
            generate(shell, &mut Cli::command(), "iidy-rs", &mut io::stdout());
            debug!("Completion for {shell:?}");
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
            debug!("CLI options: {cli:?}");

            // TODO: see if we can get rid of this global color setup.
            // I think it was introduced when implementing yaml error handling

            // Initialize color context early for global access
            let theme = match cli.global_opts.theme {
                iidy::cli::Theme::Auto => TerminalTheme::Auto,
                iidy::cli::Theme::Light => TerminalTheme::Light,
                iidy::cli::Theme::Dark => TerminalTheme::Dark,
                iidy::cli::Theme::HighContrast => TerminalTheme::HighContrast,
            };
            ColorContext::init_global(cli.global_opts.color, theme);

            handle_command(cli)
        }
        Err(e)
            if matches!(
                e.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) =>
        {
            e.print().unwrap();
            std::process::exit(0); // override exit code 2 -> 0
        }
        Err(e) => {
            e.print().unwrap();
            std::process::exit(1); // non-help errors still exit with error
        }
    }
}
