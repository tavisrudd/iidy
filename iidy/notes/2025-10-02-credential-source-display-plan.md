# Plan: Display IAM Credential Source in Command Metadata

## Problem
When users see "Current IAM Principal: arn:aws:iam::123456789012:user/foobar" in command metadata output, they don't know whether these credentials came from:
- Environment variables (AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY)
- A named profile from ~/.aws/credentials
- AssumeRole
- EC2 instance metadata
- ECS container credentials

This is especially confusing when a profile is specified via `--profile` or `Profile:` in stack-args.yaml but environment variables silently override it.

## Goal
Display the credential source alongside the IAM principal in command metadata output with full traceability of configuration sources:
```
Current IAM Principal:   arn:aws:iam::123456789012:user/foobar
Credential Source:       environment variables (AWS_ACCESS_KEY_ID) (overriding profile 'production' (from stack-args.yaml))
```

## Research Findings

### Current Implementation
1. **IAM principal retrieval**: `src/output/aws_conversion.rs:89-105` - `get_current_iam_principal()`
   - Calls STS GetCallerIdentity to get the ARN
   - Returns only the ARN string
   - Uses context's AWS config (correct)

2. **Display location**: `src/output/renderers/interactive.rs:948`
   - Shows just `current_iam_principal` field from `CommandMetadata`

3. **Data structure**: `src/output/data.rs:30-40`
   - `current_iam_principal: String` - stores just the ARN
   - No credential source information

### AWS SDK Limitation
The Rust AWS SDK **does not expose** which credential provider was actually used. The credential provider chain is opaque - you configure it, it resolves credentials, but there's no API to ask "which provider gave me these credentials?"

## Solution Approach

Since we can't query the AWS SDK for the provider source, we need to **detect it ourselves** by checking the same sources in the same order as the AWS SDK credential chain.

### Detection Strategy

**Key Insight**: We need to track:
1. **Credential sources** - where credentials come from (env vars, profile files, container metadata, etc.)
2. **Setting sources** - where the profile/role settings came from (CLI flags, stack-args.yaml, env vars)
3. **All configured sources** - not just the winning one, so we can show override warnings

**AWS SDK Credential Provider Chain Order** (highest to lowest precedence):
1. **Environment variables**: `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` (optionally `AWS_SESSION_TOKEN`)
2. **Web identity token**: `AWS_WEB_IDENTITY_TOKEN_FILE` + `AWS_ROLE_ARN`
3. **Container credentials**: `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI` or `AWS_CONTAINER_CREDENTIALS_FULL_URI`
4. **Profile from files**: `~/.aws/credentials` or `~/.aws/config` (selected by profile setting)
5. **EC2 IMDS**: Instance metadata service (requires network probe - skip for performance)

**Profile Setting Resolution** (iidy-specific, highest to lowest precedence):
1. CLI flag: `--profile <name>`
2. Stack-args: `Profile: <name>` in stack-args.yaml
3. Environment variable: `AWS_PROFILE`
4. Default: `"default"`

**AssumeRoleArn Setting Resolution** (iidy-specific):
1. CLI flag: `--assume-role-arn <arn>`
2. Stack-args: `AssumeRoleARN: <arn>` in stack-args.yaml

### Information Available from GetCallerIdentity

We already call `STS::GetCallerIdentity` to get the IAM principal ARN. The response contains additional information at **zero additional cost**:

**ARN format reveals credential type**:
- `arn:aws:iam::123:user/username` → IAM user credentials
- `arn:aws:sts::123:assumed-role/RoleName/iidy` → AssumeRole with session name "iidy" (our explicit AssumeRoleARN)
- `arn:aws:sts::123:assumed-role/RoleName/other` → AssumeRole from profile's role_arn setting
- `arn:aws:iam::123:root` → Root account credentials

**UserId format** (also in response):
- `AIDA...` → IAM user
- `AROA...` → Assumed role from user
- `ASIA...` → Temporary credentials

### Profile Type Detection

Parse `~/.aws/config` to detect if a profile does internal role assumption:
- **Performance**: ~1-5ms (file read + simple INI parsing)
- **Error handling**: Gracefully degrade to not showing role info if file unreadable
- **Value**: Shows users when profile itself does role assumption vs. our explicit AssumeRoleARN

### Display Format (Option C)

Show the active credential source AND what it's overriding, with full traceability:

```
Current IAM Principal:   arn:aws:iam::123456789012:user/foobar
Credential Source:       environment variables (AWS_ACCESS_KEY_ID) (overriding profile 'production' (from stack-args.yaml))
```

**More examples**:
- Simple case: `profile 'production' (from --profile)`
- Profile with role: `profile 'production' (from stack-args.yaml, assumes role DeployRole)`
- Our AssumeRole: `AssumeRole DeployRole (from --assume-role-arn) via profile 'default' (default)`
- Complex override: `environment variables (AWS_ACCESS_KEY_ID + AWS_SESSION_TOKEN) (overriding AssumeRole DeployRole (from stack-args.yaml) via profile 'production' (from stack-args.yaml))`

## Implementation Plan

### Phase 1: Create Credential Source Types

**New file**: `src/aws/credential_source.rs`

```rust
/// Where the profile setting came from
#[derive(Clone, Debug)]
pub enum ProfileSource {
    CliFlag,          // --profile
    StackArgs,        // Profile: in stack-args.yaml
    AwsProfileEnvVar, // AWS_PROFILE environment variable
    Default,          // "default" (nothing specified anywhere)
}

impl ProfileSource {
    pub fn display_name(&self) -> &str {
        match self {
            Self::CliFlag => "from --profile",
            Self::StackArgs => "from stack-args.yaml",
            Self::AwsProfileEnvVar => "from AWS_PROFILE env var",
            Self::Default => "default",
        }
    }
}

/// Where the AssumeRoleArn setting came from
#[derive(Clone, Debug)]
pub enum AssumeRoleSource {
    CliFlag,   // --assume-role-arn
    StackArgs, // AssumeRoleARN: in stack-args.yaml
}

impl AssumeRoleSource {
    pub fn display_name(&self) -> &str {
        match self {
            Self::CliFlag => "from --assume-role-arn",
            Self::StackArgs => "from stack-args.yaml",
        }
    }
}

/// Credential source with full provenance tracking
#[derive(Clone, Debug)]
pub enum CredentialSource {
    /// Static credentials from AWS_ACCESS_KEY_ID + AWS_SECRET_ACCESS_KEY
    EnvironmentVariablesStatic,

    /// Temporary credentials from env vars (includes AWS_SESSION_TOKEN)
    EnvironmentVariablesTemporary,

    /// Profile from ~/.aws/credentials or ~/.aws/config
    Profile {
        name: String,
        source: ProfileSource,
        /// role_arn defined in ~/.aws/config for this profile
        profile_role_arn: Option<String>,
    },

    /// AssumeRole wraps base credentials (transformation, not a source)
    AssumeRole {
        base_source: Box<CredentialSource>,
        role_arn: String,
        source: AssumeRoleSource,
    },

    /// ECS container credentials (AWS_CONTAINER_CREDENTIALS_RELATIVE_URI)
    ContainerCredentialsEcs,

    /// Generic container credentials (AWS_CONTAINER_CREDENTIALS_FULL_URI)
    ContainerCredentialsGeneric,

    /// Web identity token (AWS_WEB_IDENTITY_TOKEN_FILE + AWS_ROLE_ARN)
    WebIdentityToken,

    /// EC2 instance metadata (not probed for performance reasons)
    InstanceMetadata,

    /// Unable to determine specific source
    Unknown,
}

impl CredentialSource {
    pub fn display_name(&self) -> String {
        match self {
            Self::EnvironmentVariablesStatic => {
                "environment variables (AWS_ACCESS_KEY_ID)".to_string()
            }
            Self::EnvironmentVariablesTemporary => {
                "environment variables (AWS_ACCESS_KEY_ID + AWS_SESSION_TOKEN)".to_string()
            }
            Self::Profile { name, source, profile_role_arn } => {
                let source_str = source.display_name();
                if let Some(role) = profile_role_arn {
                    let short_role = role.split('/').last().unwrap_or(role);
                    format!("profile '{}' ({}, assumes role {})", name, source_str, short_role)
                } else {
                    format!("profile '{}' ({})", name, source_str)
                }
            }
            Self::AssumeRole { base_source, role_arn, source } => {
                let short_arn = role_arn.split('/').last().unwrap_or(role_arn);
                let source_str = source.display_name();
                format!(
                    "AssumeRole {} ({}) via {}",
                    short_arn,
                    source_str,
                    base_source.display_name()
                )
            }
            Self::ContainerCredentialsEcs => "ECS container credentials".to_string(),
            Self::ContainerCredentialsGeneric => "container credentials".to_string(),
            Self::WebIdentityToken => "web identity token".to_string(),
            Self::InstanceMetadata => "EC2 instance metadata".to_string(),
            Self::Unknown => "AWS SDK default chain".to_string(),
        }
    }
}

/// Stack of credential sources sorted by precedence (highest first).
/// The first source is the active one; remaining sources were configured but overridden.
#[derive(Clone, Debug)]
pub struct CredentialSourceStack {
    sources: Vec<CredentialSource>,
}

impl CredentialSourceStack {
    pub fn new(sources: Vec<CredentialSource>) -> Self {
        Self { sources }
    }

    /// The credential source that will actually be used (highest precedence)
    pub fn active(&self) -> &CredentialSource {
        self.sources.first().unwrap_or(&CredentialSource::Unknown)
    }

    /// Sources that were configured but overridden by higher-precedence ones
    pub fn overridden(&self) -> &[CredentialSource] {
        if self.sources.len() > 1 {
            &self.sources[1..]
        } else {
            &[]
        }
    }

    /// Generate display string with override warnings (Option C format)
    pub fn display_name(&self) -> String {
        let active = self.active();
        let overridden = self.overridden();

        if overridden.is_empty() {
            active.display_name()
        } else {
            let overridden_names: Vec<String> = overridden
                .iter()
                .map(|s| s.display_name())
                .collect();

            format!(
                "{} (overriding {})",
                active.display_name(),
                overridden_names.join(" and ")
            )
        }
    }
}

/// Context needed to detect credential sources with full provenance
pub struct CredentialDetectionContext {
    pub cli_profile: Option<String>,
    pub stack_args_profile: Option<String>,
    pub cli_assume_role_arn: Option<String>,
    pub stack_args_assume_role_arn: Option<String>,
}

/// Parse ~/.aws/config to determine if a profile assumes a role internally
fn get_profile_role_arn(profile_name: &str) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let config_path = std::path::Path::new(&home).join(".aws/config");

    // Read config file (gracefully handle errors)
    let content = std::fs::read_to_string(config_path).ok()?;

    // Simple INI parser - find [profile name] section
    let section = if profile_name == "default" {
        "[default]".to_string()
    } else {
        format!("[profile {}]", profile_name)
    };

    let in_section = content.lines()
        .skip_while(|line| !line.starts_with(&section))
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['));

    for line in in_section {
        if let Some(stripped) = line.trim_start().strip_prefix("role_arn") {
            if let Some(value) = stripped.trim_start().strip_prefix('=') {
                return Some(value.trim().to_string());
            }
        }
    }

    None
}

/// Detect all configured credential sources in AWS SDK precedence order.
/// Returns a stack with the active source first, followed by any overridden sources.
pub fn detect_credential_sources(ctx: &CredentialDetectionContext) -> CredentialSourceStack {
    let mut sources = Vec::new();

    // 1. Environment variables (highest precedence - always wins if present)
    if std::env::var("AWS_ACCESS_KEY_ID").is_ok()
        && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok() {
        let is_temporary = std::env::var("AWS_SESSION_TOKEN").is_ok();
        sources.push(if is_temporary {
            CredentialSource::EnvironmentVariablesTemporary
        } else {
            CredentialSource::EnvironmentVariablesStatic
        });
    }

    // 2. Web identity token
    if std::env::var("AWS_WEB_IDENTITY_TOKEN_FILE").is_ok()
        && std::env::var("AWS_ROLE_ARN").is_ok() {
        sources.push(CredentialSource::WebIdentityToken);
    }

    // 3. Container credentials
    if std::env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI").is_ok() {
        sources.push(CredentialSource::ContainerCredentialsEcs);
    } else if std::env::var("AWS_CONTAINER_CREDENTIALS_FULL_URI").is_ok() {
        sources.push(CredentialSource::ContainerCredentialsGeneric);
    }

    // 4. Profile - determine name AND source
    let (profile_name, profile_source) = if let Some(ref name) = ctx.cli_profile {
        (name.clone(), ProfileSource::CliFlag)
    } else if let Some(ref name) = ctx.stack_args_profile {
        (name.clone(), ProfileSource::StackArgs)
    } else if let Ok(name) = std::env::var("AWS_PROFILE") {
        (name, ProfileSource::AwsProfileEnvVar)
    } else {
        ("default".to_string(), ProfileSource::Default)
    };

    // OPTIMIZATION: Only parse ~/.aws/config if profile will actually be used
    // i.e., no higher-precedence credential sources (env vars, web identity, container) were found
    // This applies to both:
    // - Explicit profiles (from --profile, stack-args, or AWS_PROFILE env var)
    // - Default profile (when nothing is specified anywhere)
    // If profile is overridden, we skip the expensive file I/O (1-5ms)
    let profile_role_arn = if sources.is_empty() {
        get_profile_role_arn(&profile_name)  // Parse only when profile will be used
    } else {
        None  // Profile is overridden, skip file I/O
    };

    let base_source = CredentialSource::Profile {
        name: profile_name,
        source: profile_source,
        profile_role_arn,
    };

    // 5. AssumeRole wrapper - determine role AND source
    let final_source = if let Some(ref role_arn) = ctx.cli_assume_role_arn {
        CredentialSource::AssumeRole {
            base_source: Box::new(base_source),
            role_arn: role_arn.clone(),
            source: AssumeRoleSource::CliFlag,
        }
    } else if let Some(ref role_arn) = ctx.stack_args_assume_role_arn {
        CredentialSource::AssumeRole {
            base_source: Box::new(base_source),
            role_arn: role_arn.clone(),
            source: AssumeRoleSource::StackArgs,
        }
    } else {
        base_source
    };

    // Always add the profile/assume-role source as fallback
    sources.push(final_source);

    // Note: We don't probe EC2 IMDS for performance reasons (100-1000ms network call)

    CredentialSourceStack::new(sources)
}
```

**Update**: `src/aws/mod.rs` - Add module declaration:
```rust
mod credential_source;
pub use credential_source::{
    CredentialSource,
    CredentialSourceStack,
    CredentialDetectionContext,
    ProfileSource,
    AssumeRoleSource,
    detect_credential_sources
};
```

### Phase 2: Update AWS Config Creation

**File**: `src/aws/mod.rs:95-138` - Update `config_from_merged_settings()`

```rust
pub async fn config_from_merged_settings(
    merged_settings: &AwsSettings,
    detection_ctx: &CredentialDetectionContext,
) -> Result<(SdkConfig, CredentialSourceStack)> {
    // Detect credential sources BEFORE loading config
    let credential_sources = detect_credential_sources(detection_ctx);

    // ... existing AWS SDK config loading code ...

    let config = builder.build();

    Ok((config, credential_sources))
}
```

**Note**: This function's signature changes - need to pass `CredentialDetectionContext` which requires unmerged settings.

### Phase 3: Update Stack Args Loading

**File**: `src/cfn/stack_args.rs:94-215` - Update `load_stack_args()`

```rust
pub async fn load_stack_args(
    argsfile: &str,
    environment: &str,
    operation: &CfnOperation,
    cli_aws_settings: &AwsSettings,
) -> Result<(StackArgs, aws_config::SdkConfig, CredentialSourceStack)> {
    // ... existing code to load and parse stack-args.yaml ...

    // Extract AWS settings from argsfile (line 122-127)
    let argsfile_aws_settings = AwsSettings {
        profile: value.get("Profile").and_then(|v| v.as_str()).map(|s| s.to_string()),
        region: value.get("Region").and_then(|v| v.as_str()).map(|s| s.to_string()),
        assume_role_arn: value.get("AssumeRoleARN").and_then(|v| v.as_str()).map(|s| s.to_string()),
    };

    // Merge AWS settings (CLI overrides argsfile) (line 129-134)
    let merged_aws_settings = AwsSettings {
        profile: cli_aws_settings.profile.clone()
            .or_else(|| argsfile_aws_settings.profile.clone()),
        region: cli_aws_settings.region.clone()
            .or_else(|| argsfile_aws_settings.region.clone()),
        assume_role_arn: cli_aws_settings.assume_role_arn.clone()
            .or_else(|| argsfile_aws_settings.assume_role_arn.clone()),
    };

    // NEW: Create detection context with unmerged settings for provenance tracking
    let detection_ctx = CredentialDetectionContext {
        cli_profile: cli_aws_settings.profile.clone(),
        stack_args_profile: argsfile_aws_settings.profile.clone(),
        cli_assume_role_arn: cli_aws_settings.assume_role_arn.clone(),
        stack_args_assume_role_arn: argsfile_aws_settings.assume_role_arn.clone(),
    };

    // Configure AWS (line 137)
    let (aws_config, credential_sources) = config_from_merged_settings(
        &merged_aws_settings,
        &detection_ctx,
    ).await?;

    // ... rest of existing implementation ...

    Ok((stack_args, aws_config, credential_sources))
}
```

### Phase 4: Update CfnContext

**File**: `src/cfn/mod.rs` - Update `CfnContext` struct

```rust
use crate::aws::CredentialSourceStack;

pub struct CfnContext {
    pub client: Client,
    pub aws_config: aws_config::SdkConfig,
    pub credential_sources: CredentialSourceStack,  // NEW
    pub time_provider: Arc<dyn TimeProvider>,
    pub start_time: DateTime<Utc>,
    pub token_info: TokenInfo,
    pub used_tokens: Arc<Mutex<Vec<TokenInfo>>>,
}
```

**Update**: `CfnContext::new()` - Add parameter and field initialization:
```rust
pub async fn new(
    client: Client,
    aws_config: aws_config::SdkConfig,
    credential_sources: CredentialSourceStack,  // NEW
    time_provider: Arc<dyn TimeProvider>,
    token_info: TokenInfo,
) -> Result<Self> {
    Ok(Self {
        client,
        aws_config,
        credential_sources,  // NEW
        time_provider,
        start_time: Utc::now(),
        token_info,
        used_tokens: Arc::new(Mutex::new(Vec::new())),
    })
}
```

**Update**: `create_context_from_config()` signature (line 210-233):
```rust
pub async fn create_context_from_config(
    aws_config: aws_config::SdkConfig,
    credential_sources: CredentialSourceStack,  // NEW
    operation: CfnOperation,
    client_request_token: TokenInfo,
) -> Result<CfnContext> {
    // ... region validation ...

    let client = Client::new(&aws_config);
    let time_provider: Arc<dyn TimeProvider> = if operation.is_read_only() {
        Arc::new(SystemTimeProvider::new())
    } else {
        Arc::new(ReliableTimeProvider::new())
    };

    CfnContext::new(
        client,
        aws_config,
        credential_sources,  // NEW
        time_provider,
        client_request_token,
    ).await
}
```

### Phase 5: Update Command Macro

**File**: `src/cfn/mod.rs:107-158` - Update `run_command_handler_with_stack_args!` macro

Update stack args destructuring (around line 121):
```rust
let (stack_args, aws_config, credential_sources) = match load_stack_args(
    &argsfile,
    &environment,
    &$operation,
    &cli_aws_settings,
).await {
    Ok(result) => result,
    Err(e) => {
        // ... error handling ...
    }
};
```

Update context creation call (around line 140):
```rust
let context = create_context_from_config(
    aws_config,
    credential_sources,  // NEW
    $operation,
    primary_token,
).await?;
```

### Phase 6: Update CommandMetadata

**File**: `src/output/data.rs:30-40` - Add field:

```rust
pub struct CommandMetadata {
    pub iidy_environment: String,
    pub region: String,
    pub profile: Option<String>,
    pub cli_arguments: HashMap<String, String>,
    pub iam_service_role: Option<String>,
    pub current_iam_principal: String,
    pub credential_source: String,  // NEW: human-readable description with provenance
    pub iidy_version: String,
    pub primary_token: TokenInfo,
    pub derived_tokens: Vec<TokenInfo>,
}
```

**File**: `src/output/aws_conversion.rs:72-85` - Update construction:

```rust
pub async fn create_command_metadata(
    context: &CfnContext,
    environment: &str,
    opts: &NormalizedAwsOpts,
    stack_args: &StackArgs,
    cli_args: HashMap<String, String>,
) -> Result<CommandMetadata> {
    // ... existing code to get current_iam_principal, tokens, etc. ...

    Ok(CommandMetadata {
        iidy_environment: environment.to_string(),
        region: context.aws_config.region()
            .expect("Region must be configured - validated in create_context_from_config")
            .as_ref()
            .to_string(),
        profile: opts.profile.clone(),
        cli_arguments,
        iam_service_role: stack_args.role_arn.clone(),
        current_iam_principal,
        credential_source: context.credential_sources.display_name(),  // NEW
        iidy_version: env!("CARGO_PKG_VERSION").to_string(),
        primary_token,
        derived_tokens,
    })
}
```

### Phase 7: Update Renderers

**File**: `src/output/renderers/interactive.rs:948` - Add display after IAM principal:

```rust
self.print_section_entry(
    "Current IAM Principal:",
    &data.current_iam_principal.color(self.theme.muted).to_string()
)?;
self.print_section_entry(
    "Credential Source:",
    &data.credential_source.color(self.theme.muted).to_string()
)?;
```

**File**: `src/output/renderers/plain.rs` - Similar update

**File**: `src/output/renderers/json.rs` - Automatically includes new field in JSON serialization

### Phase 8: Update All Test Fixtures

Update all test code that constructs `CommandMetadata`:

**Files to update**:
- `src/output/test_data.rs:12-40`
- `src/output/fixtures/mod.rs:71-95`
- `src/output/renderers/json.rs:340-360`
- `tests/output_unit_tests.rs:18-32`, `124-138`
- `tests/keyboard_integration_tests.rs:44-59`
- `tests/dynamic_output_manager_tests.rs:43-62`
- All YAML test fixtures in `tests/fixtures/`

Add field to each:
```rust
credential_source: "profile 'test' (default)".to_string(),
```

**Files constructing CfnContext**:
- Need to pass `credential_sources` parameter
- Create a test helper or use a simple stack:
```rust
let test_sources = CredentialSourceStack::new(vec![
    CredentialSource::Profile {
        name: "test".to_string(),
        source: ProfileSource::Default,
        profile_role_arn: None,
    }
]);
```

## Testing Strategy

### Unit Tests

**File**: `src/aws/credential_source.rs` - Add tests module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_vars_static_only() {
        // Temporarily set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY
        // Should detect: EnvironmentVariablesStatic
    }

    #[test]
    fn test_env_vars_temporary() {
        // Set AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_SESSION_TOKEN
        // Should detect: EnvironmentVariablesTemporary
    }

    #[test]
    fn test_env_vars_override_cli_profile() {
        // Set env vars AND cli_profile
        // Should show override warning
    }

    #[test]
    fn test_env_vars_override_stack_args_profile() {
        // Set env vars AND stack_args_profile
        // Should show override warning with "(from stack-args.yaml)"
    }

    #[test]
    fn test_cli_profile_only() {
        // Only cli_profile set
        // Should show: "profile 'foo' (from --profile)"
    }

    #[test]
    fn test_stack_args_profile_only() {
        // Only stack_args_profile set
        // Should show: "profile 'bar' (from stack-args.yaml)"
    }

    #[test]
    fn test_aws_profile_env_var() {
        // Set AWS_PROFILE env var
        // Should show: "profile 'baz' (from AWS_PROFILE env var)"
    }

    #[test]
    fn test_default_profile() {
        // Nothing set
        // Should show: "profile 'default' (default)"
    }

    #[test]
    fn test_cli_assume_role_with_stack_args_profile() {
        // cli_assume_role_arn + stack_args_profile
        // Should show: "AssumeRole MyRole (from --assume-role-arn) via profile 'prod' (from stack-args.yaml)"
    }

    #[test]
    fn test_stack_args_assume_role_with_cli_profile() {
        // stack_args_assume_role_arn + cli_profile
        // Should show: "AssumeRole MyRole (from stack-args.yaml) via profile 'prod' (from --profile)"
    }

    #[test]
    fn test_container_credentials_ecs() {
        // Set AWS_CONTAINER_CREDENTIALS_RELATIVE_URI
        // Should detect: ContainerCredentialsEcs
    }

    #[test]
    fn test_web_identity_token() {
        // Set AWS_WEB_IDENTITY_TOKEN_FILE + AWS_ROLE_ARN
        // Should detect: WebIdentityToken
    }

    #[test]
    fn test_profile_role_arn_parsing() {
        // Create temporary ~/.aws/config with role_arn
        // Verify get_profile_role_arn() extracts it correctly
    }
}
```

### Integration Tests

1. **Snapshot tests**: Update all snapshot expectations to include `Credential Source:` field
2. **End-to-end tests**: Test with actual AWS config files (in test environment)

### Manual Testing

Test scenarios to verify in real environment:

1. **Env vars only**: Set `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`
   - Expected: `environment variables (AWS_ACCESS_KEY_ID)`

2. **Env vars override CLI profile**: Set env vars + `--profile production`
   - Expected: `environment variables (AWS_ACCESS_KEY_ID) (overriding profile 'production' (from --profile))`

3. **Env vars override stack-args profile**: Set env vars + `Profile: production` in stack-args.yaml
   - Expected: `environment variables (AWS_ACCESS_KEY_ID) (overriding profile 'production' (from stack-args.yaml))`

4. **CLI profile only**: `--profile staging`
   - Expected: `profile 'staging' (from --profile)`

5. **Stack-args profile only**: `Profile: staging` in stack-args.yaml
   - Expected: `profile 'staging' (from stack-args.yaml)`

6. **Profile with internal role**: Profile that has `role_arn` in `~/.aws/config`
   - Expected: `profile 'production' (from --profile, assumes role DeployRole)`

7. **CLI AssumeRole wrapping profile**: `--assume-role-arn arn:aws:iam::123:role/MyRole --profile default`
   - Expected: `AssumeRole MyRole (from --assume-role-arn) via profile 'default' (from --profile)`

8. **Stack-args AssumeRole wrapping profile**: Both in stack-args.yaml
   - Expected: `AssumeRole MyRole (from stack-args.yaml) via profile 'default' (default)`

## Performance Analysis

| Operation | Time | Risk | Mitigation |
|-----------|------|------|------------|
| Check env vars (`std::env::var`) | < 1μs | None | N/A |
| Read `~/.aws/config` | 1-5ms | File not found, permission denied | Gracefully return None; **only parsed if profile will be used** |
| Parse INI file | < 1ms | Malformed syntax | Simple parser, ignore errors |
| STS GetCallerIdentity | **Already called** | Network failure | Already handled in existing code |
| EC2 IMDS probe | 100-1000ms | Timeout | **Skip entirely** |

**Total added overhead**:
- **~1-6ms** when profile is used (dominated by ~/.aws/config file I/O)
- **< 1μs** when env vars/container credentials override profile (file I/O skipped)

**Error handling**: All operations fail gracefully - worst case falls back to less detailed display.

## Migration Considerations

### Breaking Changes

All these changes are internal - no breaking changes to CLI interface or stack-args.yaml format.

**Internal signature changes**:
1. `config_from_merged_settings()` - gains `CredentialDetectionContext` parameter, returns tuple
2. `load_stack_args()` - returns 3-tuple instead of 2-tuple
3. `create_context_from_config()` - gains `CredentialSourceStack` parameter
4. `CfnContext::new()` - gains `CredentialSourceStack` parameter
5. `CfnContext` struct - gains `credential_sources` field
6. `CommandMetadata` struct - gains `credential_source` field

**Test updates required**:
- All `CommandMetadata` construction sites need new field
- All `CfnContext` construction sites need new parameter
- Snapshot tests need re-approval with `cargo insta accept`

### Backwards Compatibility

- **Output format changes**: Interactive and plain renderers gain one line, JSON gains one field
- **No config file changes**: All existing stack-args.yaml files work unchanged
- **No CLI changes**: All existing CLI invocations work unchanged

## Risks & Alternatives

### Risk: Detection Mismatch

Our detection logic might not perfectly match AWS SDK's actual resolution in edge cases.

**Examples of potential mismatches**:
- SSO profiles (we'd show generic profile, SDK uses SSO provider)
- Custom credential process (we can't detect it from config file easily)
- Instance metadata when running on EC2 (we don't probe IMDS)

**Mitigation**:
- Use conservative labeling - default to showing profile info we know about
- Future enhancement: Cross-check ARN format from GetCallerIdentity
- Users get *more* information than before, even if not 100% precise

### Risk: File I/O Performance

Reading `~/.aws/config` adds 1-6ms latency.

**Mitigation**:
- Acceptable overhead for command-line tool
- Only reads once per command execution
- Gracefully handles missing/unreadable files

### Alternative: Only Show Basic Source

Don't track provenance (CLI vs stack-args vs env var), just show credential type.

**Rejected**: The provenance is exactly what users need to debug "why is my --profile being ignored?"

### Alternative: Skip Profile Role Detection

Don't parse `~/.aws/config` for role_arn.

**Consideration**: Saves 1-5ms but loses valuable information. Recommended to include it.

## Effort Estimate

- Phase 1 (credential source types): 2-3 hours
- Phase 2-3 (AWS config + stack args): 1-2 hours
- Phase 4-5 (CfnContext + macro): 1-2 hours
- Phase 6 (CommandMetadata): 30 min
- Phase 7 (renderers): 30 min
- Phase 8 (test fixture updates): 2-3 hours
- Unit tests: 1-2 hours
- Manual testing: 1 hour

**Total**: ~9-14 hours

## Future Enhancements

1. **ARN validation**: Cross-check detected source against GetCallerIdentity ARN format
   - If ARN is `assumed-role/*/iidy` → validate AssumeRole was detected
   - If ARN is `user/*` → validate static credentials detected

2. **Credential expiry display**: For temporary credentials, parse and show expiry time
   - Could decode session token (if available) to extract expiration
   - Show warning if credentials expire soon

3. **MFA status**: Detect if MFA was used for AssumeRole
   - Parse `~/.aws/config` for `mfa_serial`
   - Show in credential source display

4. **SSO profile detection**: Parse `~/.aws/config` for `sso_*` settings
   - Show "profile 'prod' (from --profile, uses AWS SSO)"

5. **Credential process detection**: Parse `~/.aws/config` for `credential_process`
   - Show "profile 'prod' (from --profile, uses credential process)"

6. **Source-specific help**: When credential errors occur, link to relevant docs
   - Env vars → "Check AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY"
   - Profile → "Check ~/.aws/credentials and ~/.aws/config"
   - Container → "Check ECS task role"
