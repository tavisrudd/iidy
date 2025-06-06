use std::io;
use log::debug;
use env_logger;
use clap::{CommandFactory, Parser, error::ErrorKind};
use clap_complete::{Shell, generate};

use iidy::{cfn, cli::{Cli, Commands, RenderArgs}, yaml::preprocess_yaml_with_spec};
use anyhow::Result;
use std::fs;
use std::path::Path;
mod demo;
use tokio::runtime::Runtime;

fn handle_command(cli: Cli) {
    let rt = Runtime::new().expect("failed to create tokio runtime");
    match cli.command {
        Commands::CreateStack(args) => {
            let normalized_opts = cli.aws_opts.normalize();
            if let Err(e) = rt.block_on(cfn::create_stack::create_stack(&normalized_opts, &args)) {
                eprintln!("error creating stack: {e:?}");
            }
        }
        Commands::UpdateStack(args) => {
            let normalized_opts = cli.aws_opts.normalize();
            if let Err(e) = rt.block_on(cfn::update_stack::update_stack(&normalized_opts, &args)) {
                eprintln!("error updating stack: {e:?}");
            }
        }
        Commands::CreateOrUpdate(args) => {
            let normalized_opts = cli.aws_opts.normalize();
            if let Err(e) = rt.block_on(cfn::create_or_update::create_or_update(
                &normalized_opts,
                &args,
            )) {
                eprintln!("error creating or updating stack: {e:?}");
            }
        }
        Commands::EstimateCost(args) => {
            let normalized_opts = cli.aws_opts.normalize();
            if let Err(e) = rt.block_on(cfn::estimate_cost::estimate_cost(&normalized_opts, &args)) {
                eprintln!("error estimating cost: {e:?}");
            }
        }
        Commands::DummySpacer => {}
        Commands::CreateChangeset(args) => {
            let normalized_opts = cli.aws_opts.normalize();
            if let Err(e) = rt.block_on(cfn::create_changeset::create_changeset(
                &normalized_opts,
                &args,
            )) {
                eprintln!("error creating change set: {e:?}");
            }
        }
        Commands::ExecChangeset(args) => {
            let normalized_opts = cli.aws_opts.normalize();
            if let Err(e) = rt.block_on(cfn::exec_changeset::exec_changeset(&normalized_opts, &args)) {
                eprintln!("error executing change set: {e:?}");
            }
        }
        Commands::DummySpacer2 => {}
        Commands::DescribeStack(args) => {
            let normalized_opts = cli.aws_opts.normalize();
            if let Err(e) = rt.block_on(cfn::describe_stack::describe_stack(&normalized_opts, &args)) {
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
            let normalized_opts = cli.aws_opts.normalize();
            if let Err(e) = rt.block_on(cfn::watch_stack::watch_stack(&normalized_opts, &args)) {
                eprintln!("error watching stack: {e:?}");
            }
        }
        Commands::DeleteStack(args) => {
            let normalized_opts = cli.aws_opts.normalize();
            if let Err(e) = rt.block_on(cfn::delete_stack::delete_stack(&normalized_opts, &args)) {
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
        Commands::Render(args) => {
            if let Err(e) = rt.block_on(handle_render_command(&args)) {
                eprintln!("error rendering template: {e:?}");
            }
        }
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

async fn handle_render_command(args: &RenderArgs) -> Result<()> {
    // Read the template file
    let template_content = fs::read_to_string(&args.template)?;
    
    // Get the base location from the template file path for relative imports
    let base_location = &args.template;
    
    // Process the YAML with the new preprocessing system using specified YAML spec
    let processed_value = preprocess_yaml_with_spec(&template_content, base_location, &args.yaml_spec).await?;
    
    // Apply query selector if provided
    let output_value = if let Some(query) = &args.query {
        apply_query_to_value(processed_value, query)?
    } else {
        processed_value
    };
    
    // Format output based on requested format
    let formatted_output = match args.format.as_str() {
        "json" => serde_json::to_string_pretty(&output_value)?,
        "yaml" | "yml" => serde_yaml::to_string(&output_value)?,
        _ => return Err(anyhow::anyhow!("Unsupported format: {}. Use 'yaml' or 'json'", args.format)),
    };
    
    // Output to file or stdout
    if args.outfile == "stdout" || args.outfile == "-" {
        println!("{}", formatted_output);
    } else {
        // Check if file exists and handle overwrite logic
        if Path::new(&args.outfile).exists() && !args.overwrite {
            return Err(anyhow::anyhow!(
                "Output file '{}' exists. Use --overwrite to overwrite it.", 
                args.outfile
            ));
        }
        
        fs::write(&args.outfile, formatted_output)?;
        eprintln!("Template rendered to: {}", args.outfile);
    }
    
    Ok(())
}

fn apply_query_to_value(value: serde_yaml::Value, query: &str) -> Result<serde_yaml::Value> {
    // Simple query support - handles dot notation like "Resources.MyBucket"
    let parts: Vec<&str> = query.split('.').collect();
    let mut current = value;
    
    for part in parts {
        if part.is_empty() {
            continue;
        }
        
        match current {
            serde_yaml::Value::Mapping(ref map) => {
                let key = serde_yaml::Value::String(part.to_string());
                if let Some(next_value) = map.get(&key) {
                    current = next_value.clone();
                } else {
                    return Err(anyhow::anyhow!("Query path '{}' not found at key '{}'", query, part));
                }
            }
            _ => {
                return Err(anyhow::anyhow!("Cannot query '{}' on non-mapping value", part));
            }
        }
    }
    
    Ok(current)
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
