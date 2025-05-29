use atty;
use atty::Stream;
use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

const AWS_REGIONS: [&str; 26] = [
    "us-east-1",
    "us-east-2",
    "us-west-1",
    "us-west-2",
    "ca-central-1",
    "sa-east-1",
    "eu-central-1",
    "eu-west-1",
    "eu-west-2",
    "eu-west-3",
    "eu-north-1",
    "eu-south-1",
    "ap-south-1",
    "ap-south-2",
    "ap-southeast-1",
    "ap-southeast-2",
    "ap-southeast-3",
    "ap-southeast-4",
    "ap-northeast-1",
    "ap-northeast-2",
    "ap-northeast-3",
    "ap-east-1",
    "me-south-1",
    "me-central-1",
    "us-gov-west-1",
    "us-gov-east-1",
];

fn styles() -> Styles {
    if atty::is(Stream::Stdout) {
        Styles::styled()
            .header(AnsiColor::Yellow.on_default() | Effects::BOLD)
            .usage(AnsiColor::Yellow.on_default() | Effects::BOLD)
            .literal(AnsiColor::Cyan.on_default() | Effects::BOLD)
            .placeholder(AnsiColor::Cyan.on_default())
    } else {
        Styles::plain()
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "iidy-rs",
    bin_name = "iidy-rs",
    about = "CloudFormation with Confidence",
    long_about = "CloudFormation with Confidence\n\nAn acronym for \"Is it done yet?\"",
    after_help = "Status Codes:\n  Success (0)       Command successfully completed\n  Error (1)         An error was encountered while executing command\n  Cancelled (130)   User responded 'No' to iidy prompt or interrupt (CTRL-C) was received",
    version,
    arg_required_else_help = true,
    styles = styles()
)]
pub struct Cli {
    #[clap(flatten)]
    pub global_opts: GlobalOpts,

    #[clap(flatten)]
    pub aws_opts: AwsOpts,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Args)]
#[clap(next_help_heading = "Global Options")]
pub struct GlobalOpts {
    #[arg(
        short,
        long,
        global = true,
        default_value = "development",
        help = "Used to load environment based settings: AWS Profile, Region, etc."
    )]
    pub environment: String,

    #[arg(long, value_enum, global = true, default_value_t = ColorChoice::Auto, help = "Whether to color output using ANSI escape codes")]
    pub color: ColorChoice,

    #[arg(
        long,
        global = true,
        default_value_t = false,
        help = "Log debug information to stderr."
    )]
    pub debug: bool,

    #[arg(
        long,
        global = true,
        default_value_t = false,
        help = "Log full error information to stderr."
    )]
    pub log_full_error: bool,
}

#[derive(Debug, Args)]
pub struct AwsOpts {
    #[arg(
        long,
        global = true,
        help = "AWS region. Can also be set via --environment & stack-args.yaml:Region.",
        help_heading = "AWS Options",
        hide_possible_values = true,
        value_parser = clap::builder::PossibleValuesParser::new(&AWS_REGIONS)
    )]
    pub region: Option<String>,

    #[arg(
        long,
        group = "aws-auth",
        global = true,
        help_heading = "AWS Options",
        help = "AWS profile. Can also be set via --environment & stack-args.yaml:Profile. Use --profile=no-profile to override values in stack-args.yaml and use AWS_* env vars."
    )]
    pub profile: Option<String>,

    #[arg(
        long,
        group = "aws-auth",
        global = true,
        help_heading = "AWS Options",
        help = "AWS role. Can also be set via --environment & stack-args.yaml:AssumeRoleArn. Use --assume-role-arn=no-role to override values in stack-args.yaml and use AWS_* env vars."
    )]
    pub assume_role_arn: Option<String>,
    #[arg(
        long,
        global = true,
        group = "aws",
        help_heading = "AWS Options",
        help = "A unique, case-sensitive string of up to 64 ASCII characters used to ensure idempotent retries."
    )]
    pub client_request_token: Option<String>,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum TemplateFormat {
    Json,
    Yaml,
    Original,
}

impl std::fmt::Display for TemplateFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TemplateFormat::Json => "json",
            TemplateFormat::Yaml => "yaml",
            TemplateFormat::Original => "original",
        };
        write!(f, "{s}")
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub enum TemplateStageArg {
    Original,
    Processed,
}

impl std::fmt::Display for TemplateStageArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TemplateStageArg::Original => "Original",
            TemplateStageArg::Processed => "Processed",
        };
        write!(f, "{s}")
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// create a cfn stack based on stack-args.yaml
    CreateStack(StackFileArgs),
    /// update a cfn stack based on stack-args.yaml
    UpdateStack(UpdateStackArgs),
    /// create or update a cfn stack based on stack-args.yaml
    CreateOrUpdate(UpdateStackArgs),
    /// estimate aws costs based on stack-args.yaml
    EstimateCost(StackFileArgs),
    #[clap(name = "\u{2800}")]
    DummySpacer,
    /// create a cfn changeset based on stack-args.yaml
    CreateChangeset(CreateChangeSetArgs),
    /// execute a cfn changeset based on stack-args.yaml
    ExecChangeset(ExecChangeSetArgs),
    #[clap(name = "\u{2800}\u{2800}")]
    DummySpacer2,
    /// describe a stack
    DescribeStack(DescribeArgs),
    /// watch a stack that is already being created or updated
    WatchStack(WatchArgs),
    /// describe stack drift
    DescribeStackDrift(DriftArgs),
    /// delete a stack (after confirmation)
    DeleteStack(DeleteArgs),
    /// download the template of a live stack
    GetStackTemplate(GetTemplateArgs),
    /// list the ec2 instances of a live stack
    GetStackInstances(StackNameArg),
    /// list all stacks within a region
    ListStacks(ListArgs),
    #[clap(name = "\u{2800}\u{2800}\u{2800}")]
    DummySpacer3,
    /// sub commands for working with AWS SSM Parameter Store
    Param {
        #[command(subcommand)]
        command: ParamCommands,
    },
    #[clap(name = "\u{2800}\u{2800}\u{2800}\u{2800}")]
    DummySpacer4,
    /// sub commands for template approval
    TemplateApproval {
        #[command(subcommand)]
        command: ApprovalCommands,
    },
    #[clap(name = "\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}")]
    DummySpacer5,
    /// pre-process and render yaml template
    Render(RenderArgs),
    /// retrieve and print an $import value directly
    GetImport(GetImportArgs),
    /// run a demo script
    Demo(DemoArgs),
    /// lint a CloudFormation template
    LintTemplate(LintTemplateArgs),
    /// create an iidy project directory from an existing CFN stack
    ConvertStackToIidy(ConvertArgs),
    /// initialize stack-args.yaml and cfn-template.yaml
    InitStackArgs(InitStackArgs),
    #[clap(name = "\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}")]
    DummySpacer6,
    /// generate shell completion script
    Completion { shell: Option<Shell> },
}

#[derive(Args, Debug)]
pub struct StackFileArgs {
    pub argsfile: String,
    #[arg(long = "stack-name")]
    pub stack_name: Option<String>,
}

#[derive(Args, Debug)]
pub struct UpdateStackArgs {
    #[command(flatten)]
    pub base: StackFileArgs,
    #[arg(long)]
    pub lint_template: Option<bool>,
    #[arg(long)]
    pub changeset: bool,
    #[arg(long)]
    pub yes: bool,
    #[arg(long, default_value_t = true)]
    pub diff: bool,
    #[arg(long = "stack-policy-during-update")]
    pub stack_policy_during_update: Option<String>,
}

#[derive(Args, Debug)]
pub struct CreateChangeSetArgs {
    pub argsfile: String,
    pub changeset_name: Option<String>,
    #[arg(long)]
    pub watch: bool,
    #[arg(long = "watch-inactivity-timeout", default_value_t = 180)]
    pub watch_inactivity_timeout: u32,
    #[arg(long)]
    pub description: Option<String>,
    #[arg(long = "stack-name")]
    pub stack_name: Option<String>,
}

#[derive(Args, Debug)]
pub struct ExecChangeSetArgs {
    pub argsfile: String,
    pub changeset_name: String,
    #[arg(long = "stack-name")]
    pub stack_name: Option<String>,
}

#[derive(Args, Debug)]
pub struct DescribeArgs {
    pub stackname: String,
    #[arg(long, default_value_t = 50)]
    pub events: u32,
    #[arg(long)]
    pub query: Option<String>,
}

#[derive(Args, Debug)]
pub struct WatchArgs {
    pub stackname: String,
    #[arg(long = "inactivity-timeout", default_value_t = 180)]
    pub inactivity_timeout: u32,
}

#[derive(Args, Debug)]
pub struct DriftArgs {
    pub stackname: String,
    #[arg(long = "drift-cache", default_value_t = 300)]
    pub drift_cache: u32,
}

#[derive(Args, Debug)]
pub struct DeleteArgs {
    pub stackname: String,
    #[arg(long = "role-arn")]
    pub role_arn: Option<String>,
    #[arg(long = "retain-resources")]
    pub retain_resources: Vec<String>,
    #[arg(long)]
    pub yes: bool,
    #[arg(long = "fail-if-absent")]
    pub fail_if_absent: bool,
}

#[derive(Args, Debug)]
pub struct GetTemplateArgs {
    pub stackname: String,
    #[arg(long, value_enum, default_value_t = TemplateFormat::Original)]
    pub format: TemplateFormat,
    #[arg(long, value_enum, default_value_t = TemplateStageArg::Original)]
    pub stage: TemplateStageArg,
}

#[derive(Args, Debug)]
pub struct StackNameArg {
    pub stackname: String,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    #[arg(long = "tag-filter")]
    pub tag_filter: Vec<String>,
    #[arg(long = "jmespath-filter")]
    pub jmespath_filter: Option<String>,
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long)]
    pub tags: bool,
}

#[derive(Subcommand, Debug)]
pub enum ParamCommands {
    /// set a parameter value
    Set(ParamSetArgs),
    /// review a pending change
    Review(ParamPathArg),
    /// get a parameter value
    Get(ParamGetArgs),
    /// get a parameter value
    GetByPath(ParamGetByPathArgs),
    /// get a parameter's history
    GetHistory(ParamGetArgs),
}

#[derive(Args, Debug)]
pub struct ParamPathArg {
    pub path: String,
}

#[derive(Args, Debug)]
pub struct ParamSetArgs {
    pub path: String,
    pub value: String,
    #[arg(long)]
    pub message: Option<String>,
    #[arg(long)]
    pub overwrite: bool,
    #[arg(long = "with-approval")]
    pub with_approval: bool,
    #[arg(long, default_value = "SecureString")]
    pub r#type: String,
}

#[derive(Args, Debug)]
pub struct ParamGetArgs {
    pub path: String,
    #[arg(long, default_value_t = true)]
    pub decrypt: bool,
    #[arg(long, default_value = "simple")]
    pub format: String,
}

#[derive(Args, Debug)]
pub struct ParamGetByPathArgs {
    pub path: String,
    #[arg(long, default_value_t = true)]
    pub decrypt: bool,
    #[arg(long, default_value = "simple")]
    pub format: String,
    #[arg(long)]
    pub recursive: bool,
}

#[derive(Subcommand, Debug)]
pub enum ApprovalCommands {
    /// request template approval
    Request(ApprovalRequestArgs),
    /// review pending template approval request
    Review(ApprovalReviewArgs),
}

#[derive(Args, Debug)]
pub struct ApprovalRequestArgs {
    pub argsfile: String,
    #[arg(long = "lint-template", default_value_t = true)]
    pub lint_template: bool,
    #[arg(long = "lint-using-parameters")]
    pub lint_using_parameters: bool,
}

#[derive(Args, Debug)]
pub struct ApprovalReviewArgs {
    pub url: String,
    #[arg(long, default_value_t = 100)]
    pub context: u32,
}

#[derive(Args, Debug)]
pub struct RenderArgs {
    pub template: String,
    #[arg(long, default_value = "stdout")]
    pub outfile: String,
    #[arg(long, default_value = "yaml")]
    pub format: String,
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long)]
    pub overwrite: bool,
}

#[derive(Args, Debug)]
pub struct GetImportArgs {
    pub import: String,
    #[arg(long, default_value = "yaml")]
    pub format: String,
    #[arg(long)]
    pub query: Option<String>,
}

#[derive(Args, Debug)]
pub struct DemoArgs {
    pub demoscript: String,
    #[arg(long, default_value_t = 1)]
    pub timescaling: u32,
}

#[derive(Args, Debug)]
pub struct LintTemplateArgs {
    pub argsfile: String,
    #[arg(long = "use-parameters")]
    pub use_parameters: bool,
}

#[derive(Args, Debug)]
pub struct ConvertArgs {
    pub stackname: String,
    pub output_dir: String,
    #[arg(long = "move-params-to-ssm")]
    pub move_params_to_ssm: bool,
    #[arg(long, default_value_t = true)]
    pub sortkeys: bool,
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Args, Debug)]
pub struct InitStackArgs {
    #[arg(long)]
    pub force: bool,
    #[arg(long = "force-stack-args")]
    pub force_stack_args: bool,
    #[arg(long = "force-cfn-template")]
    pub force_cfn_template: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_create_stack_defaults() {
        let cli = Cli::parse_from(["iidy", "create-stack", "stack.yaml"]);
        assert_eq!(cli.global_opts.environment, "development");
        assert_eq!(cli.aws_opts.region, None);

        match cli.command {
            Commands::CreateStack(args) => {
                assert_eq!(args.argsfile, "stack.yaml");
                assert!(args.stack_name.is_none());
            }
            _ => panic!("Expected create-stack command"),
        }
    }

    #[test]
    fn parse_update_stack_with_options() {
        let cli = Cli::parse_from([
            "iidy",
            "--environment",
            "prod",
            "--region",
            "us-west-2",
            "update-stack",
            "stack.yaml",
            "--changeset",
            "--yes",
            "--stack-policy-during-update",
            "policy.json",
        ]);

        assert_eq!(cli.global_opts.environment, "prod");
        assert_eq!(cli.aws_opts.region.as_deref(), Some("us-west-2"));

        match cli.command {
            Commands::UpdateStack(args) => {
                assert_eq!(args.base.argsfile, "stack.yaml");
                assert!(args.changeset);
                assert!(args.yes);
                assert_eq!(
                    args.stack_policy_during_update.as_deref(),
                    Some("policy.json")
                );
            }
            _ => panic!("Expected update-stack command"),
        }
    }

    #[test]
    fn parse_param_set() {
        let cli = Cli::parse_from([
            "iidy",
            "param",
            "set",
            "/path/to/param",
            "value",
            "--overwrite",
            "--with-approval",
            "--type",
            "String",
        ]);

        match cli.command {
            Commands::Param { command } => match command {
                ParamCommands::Set(args) => {
                    assert_eq!(args.path, "/path/to/param");
                    assert_eq!(args.value, "value");
                    assert!(args.overwrite);
                    assert!(args.with_approval);
                    assert_eq!(args.r#type, "String");
                }
                _ => panic!("Expected ParamCommands::Set"),
            },
            _ => panic!("Expected param command"),
        }
    }

    #[test]
    fn parse_completion_default() {
        let cli = Cli::parse_from(["iidy", "completion"]);
        match cli.command {
            Commands::Completion { shell } => {
                assert_eq!(shell, None);
            }
            _ => panic!("Expected completion command"),
        }
    }

 #[test]
    fn parse_completion_shells() {
        let shells = vec![Shell::Zsh, Shell::Bash, Shell::PowerShell, Shell::Fish];
        for shell in shells {
            let cli = Cli::parse_from(["iidy", "completion", &shell.to_string()]);
            match cli.command {
                Commands::Completion { shell: s } => {
                    assert_eq!(s, Some(shell));
                }
                _ => panic!("Expected completion command"),
            }
        }
    }
}
