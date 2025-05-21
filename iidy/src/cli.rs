use clap::{Parser, Subcommand, Args, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "iidy", about = "Rust port of Unbounce/iidy", version, arg_required_else_help = true)]
pub struct Cli {
    /// Environment name used to load profile and region
    #[arg(short, long, default_value = "development", global = true)]
    pub environment: String,

    /// Unique idempotency token
    #[arg(long, global = true)]
    pub client_request_token: Option<String>,

    /// AWS region
    #[arg(long, global = true)]
    pub region: Option<String>,

    /// AWS credentials profile
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// IAM role to assume
    #[arg(long = "assume-role-arn", global = true)]
    pub assume_role_arn: Option<String>,

    /// Whether to colorize output
    #[arg(long, value_enum, default_value = "auto", global = true)]
    pub color: ColorChoice,

    /// Enable debug logging
    #[arg(long, global = true)]
    pub debug: bool,

    /// Print full error details
    #[arg(long = "log-full-error", global = true)]
    pub log_full_error: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    CreateStack(StackFileArgs),
    UpdateStack(UpdateStackArgs),
    CreateOrUpdate(UpdateStackArgs),
    EstimateCost(StackFileArgs),
    CreateChangeset(CreateChangeSetArgs),
    ExecChangeset(ExecChangeSetArgs),
    DescribeStack(DescribeArgs),
    WatchStack(WatchArgs),
    DescribeStackDrift(DriftArgs),
    DeleteStack(DeleteArgs),
    GetStackTemplate(GetTemplateArgs),
    GetStackInstances(StackNameArg),
    ListStacks(ListArgs),
    Param {
        #[command(subcommand)]
        command: ParamCommands,
    },
    TemplateApproval {
        #[command(subcommand)]
        command: ApprovalCommands,
    },
    Render(RenderArgs),
    GetImport(GetImportArgs),
    Demo(DemoArgs),
    LintTemplate(LintTemplateArgs),
    ConvertStackToIidy(ConvertArgs),
    InitStackArgs(InitStackArgs),
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
    #[arg(long, default_value = "original")]
    pub format: String,
    #[arg(long, default_value = "Original")]
    pub stage: String,
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
    Set(ParamSetArgs),
    Review(ParamPathArg),
    Get(ParamGetArgs),
    GetByPath(ParamGetByPathArgs),
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
    Request(ApprovalRequestArgs),
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
