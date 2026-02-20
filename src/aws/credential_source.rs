//! Credential source detection with full provenance tracking.
//!
//! The AWS SDK doesn't expose which credential provider was actually used,
//! so we detect it ourselves by checking the same sources in the same order
//! as the AWS SDK credential provider chain.

/// Trait for abstracting environment variable access (for testability)
pub trait EnvVarProvider {
    fn get(&self, key: &str) -> Option<String>;
}

/// Production implementation using system environment
pub struct SystemEnv;

impl EnvVarProvider for SystemEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// Test implementation using a HashMap
#[cfg(test)]
pub struct TestEnv {
    vars: std::collections::HashMap<String, String>,
}

#[cfg(test)]
impl TestEnv {
    pub fn new() -> Self {
        Self {
            vars: std::collections::HashMap::new(),
        }
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }
}

#[cfg(test)]
impl EnvVarProvider for TestEnv {
    fn get(&self, key: &str) -> Option<String> {
        self.vars.get(key).cloned()
    }
}

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
            Self::Profile {
                name,
                source,
                profile_role_arn,
            } => {
                let source_str = source.display_name();
                if let Some(role) = profile_role_arn {
                    let short_role = role.split('/').next_back().unwrap_or(role);
                    format!("profile '{name}' ({source_str}, assumes role {short_role})")
                } else {
                    format!("profile '{name}' ({source_str})")
                }
            }
            Self::AssumeRole {
                base_source,
                role_arn,
                source,
            } => {
                let short_arn = role_arn.split('/').next_back().unwrap_or(role_arn);
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
            let overridden_names: Vec<String> =
                overridden.iter().map(|s| s.display_name()).collect();

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
fn get_profile_role_arn(profile_name: &str, env: &impl EnvVarProvider) -> Option<String> {
    let home = env.get("HOME")?;
    let config_path = std::path::Path::new(&home).join(".aws/config");

    // Read config file (gracefully handle errors)
    let content = std::fs::read_to_string(config_path).ok()?;

    // Simple INI parser - find [profile name] section
    let section = if profile_name == "default" {
        "[default]".to_string()
    } else {
        format!("[profile {profile_name}]")
    };

    let in_section = content
        .lines()
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
pub fn detect_credential_sources(
    ctx: &CredentialDetectionContext,
    env: &impl EnvVarProvider,
) -> CredentialSourceStack {
    let mut sources = Vec::new();

    // 1. Environment variables (highest precedence - always wins if present)
    if env.get("AWS_ACCESS_KEY_ID").is_some() && env.get("AWS_SECRET_ACCESS_KEY").is_some() {
        let is_temporary = env.get("AWS_SESSION_TOKEN").is_some();
        sources.push(if is_temporary {
            CredentialSource::EnvironmentVariablesTemporary
        } else {
            CredentialSource::EnvironmentVariablesStatic
        });
    }

    // 2. Web identity token
    if env.get("AWS_WEB_IDENTITY_TOKEN_FILE").is_some() && env.get("AWS_ROLE_ARN").is_some() {
        sources.push(CredentialSource::WebIdentityToken);
    }

    // 3. Container credentials
    if env.get("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI").is_some() {
        sources.push(CredentialSource::ContainerCredentialsEcs);
    } else if env.get("AWS_CONTAINER_CREDENTIALS_FULL_URI").is_some() {
        sources.push(CredentialSource::ContainerCredentialsGeneric);
    }

    // 4. Profile - determine name AND source
    let (profile_name, profile_source) = if let Some(ref name) = ctx.cli_profile {
        (name.clone(), ProfileSource::CliFlag)
    } else if let Some(ref name) = ctx.stack_args_profile {
        (name.clone(), ProfileSource::StackArgs)
    } else if let Some(name) = env.get("AWS_PROFILE") {
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
        get_profile_role_arn(&profile_name, env) // Parse only when profile will be used
    } else {
        None // Profile is overridden, skip file I/O
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_vars_static_only() {
        let mut env = TestEnv::new();
        env.set("AWS_ACCESS_KEY_ID", "test-key");
        env.set("AWS_SECRET_ACCESS_KEY", "test-secret");

        let ctx = CredentialDetectionContext {
            cli_profile: None,
            stack_args_profile: None,
            cli_assume_role_arn: None,
            stack_args_assume_role_arn: None,
        };

        let stack = detect_credential_sources(&ctx, &env);

        // Should detect static env vars as highest priority
        assert!(matches!(
            stack.active(),
            CredentialSource::EnvironmentVariablesStatic
        ));
    }

    #[test]
    fn test_env_vars_temporary() {
        let mut env = TestEnv::new();
        env.set("AWS_ACCESS_KEY_ID", "test-key");
        env.set("AWS_SECRET_ACCESS_KEY", "test-secret");
        env.set("AWS_SESSION_TOKEN", "test-session");

        let ctx = CredentialDetectionContext {
            cli_profile: None,
            stack_args_profile: None,
            cli_assume_role_arn: None,
            stack_args_assume_role_arn: None,
        };

        let stack = detect_credential_sources(&ctx, &env);

        assert!(matches!(
            stack.active(),
            CredentialSource::EnvironmentVariablesTemporary
        ));
    }

    #[test]
    fn test_env_vars_override_cli_profile() {
        let mut env = TestEnv::new();
        env.set("AWS_ACCESS_KEY_ID", "test-key");
        env.set("AWS_SECRET_ACCESS_KEY", "test-secret");

        let ctx = CredentialDetectionContext {
            cli_profile: Some("production".to_string()),
            stack_args_profile: None,
            cli_assume_role_arn: None,
            stack_args_assume_role_arn: None,
        };

        let stack = detect_credential_sources(&ctx, &env);

        // Active should be env vars
        assert!(matches!(
            stack.active(),
            CredentialSource::EnvironmentVariablesStatic
        ));

        // Should have overridden profile
        assert_eq!(stack.overridden().len(), 1);
        match &stack.overridden()[0] {
            CredentialSource::Profile { name, source, .. } => {
                assert_eq!(name, "production");
                assert!(matches!(source, ProfileSource::CliFlag));
            }
            _ => panic!("Expected profile in overridden sources"),
        }

        // Display should show override
        let display = stack.display_name();
        assert!(display.contains("environment variables"));
        assert!(display.contains("overriding"));
        assert!(display.contains("production"));
        assert!(display.contains("from --profile"));
    }

    #[test]
    fn test_env_vars_override_stack_args_profile() {
        let mut env = TestEnv::new();
        env.set("AWS_ACCESS_KEY_ID", "test-key");
        env.set("AWS_SECRET_ACCESS_KEY", "test-secret");

        let ctx = CredentialDetectionContext {
            cli_profile: None,
            stack_args_profile: Some("staging".to_string()),
            cli_assume_role_arn: None,
            stack_args_assume_role_arn: None,
        };

        let stack = detect_credential_sources(&ctx, &env);

        let display = stack.display_name();
        assert!(display.contains("environment variables"));
        assert!(display.contains("overriding"));
        assert!(display.contains("staging"));
        assert!(display.contains("from stack-args.yaml"));
    }

    #[test]
    fn test_cli_profile_only() {
        let env = TestEnv::new(); // Empty env

        let ctx = CredentialDetectionContext {
            cli_profile: Some("dev".to_string()),
            stack_args_profile: None,
            cli_assume_role_arn: None,
            stack_args_assume_role_arn: None,
        };

        let stack = detect_credential_sources(&ctx, &env);

        match stack.active() {
            CredentialSource::Profile { name, source, .. } => {
                assert_eq!(name, "dev");
                assert!(matches!(source, ProfileSource::CliFlag));
            }
            _ => panic!("Expected profile credential source"),
        }

        assert_eq!(stack.overridden().len(), 0);

        let display = stack.display_name();
        assert!(display.contains("profile 'dev'"));
        assert!(display.contains("from --profile"));
    }

    #[test]
    fn test_stack_args_profile_only() {
        let env = TestEnv::new(); // Empty env

        let ctx = CredentialDetectionContext {
            cli_profile: None,
            stack_args_profile: Some("production".to_string()),
            cli_assume_role_arn: None,
            stack_args_assume_role_arn: None,
        };

        let stack = detect_credential_sources(&ctx, &env);

        match stack.active() {
            CredentialSource::Profile { name, source, .. } => {
                assert_eq!(name, "production");
                assert!(matches!(source, ProfileSource::StackArgs));
            }
            _ => panic!("Expected profile credential source"),
        }

        let display = stack.display_name();
        assert!(display.contains("profile 'production'"));
        assert!(display.contains("from stack-args.yaml"));
    }

    #[test]
    fn test_aws_profile_env_var() {
        let mut env = TestEnv::new();
        env.set("AWS_PROFILE", "myprofile");

        let ctx = CredentialDetectionContext {
            cli_profile: None,
            stack_args_profile: None,
            cli_assume_role_arn: None,
            stack_args_assume_role_arn: None,
        };

        let stack = detect_credential_sources(&ctx, &env);

        match stack.active() {
            CredentialSource::Profile { name, source, .. } => {
                assert_eq!(name, "myprofile");
                assert!(matches!(source, ProfileSource::AwsProfileEnvVar));
            }
            _ => panic!("Expected profile credential source"),
        }

        let display = stack.display_name();
        assert!(display.contains("profile 'myprofile'"));
        assert!(display.contains("from AWS_PROFILE env var"));
    }

    #[test]
    fn test_default_profile() {
        let env = TestEnv::new(); // Empty env

        let ctx = CredentialDetectionContext {
            cli_profile: None,
            stack_args_profile: None,
            cli_assume_role_arn: None,
            stack_args_assume_role_arn: None,
        };

        let stack = detect_credential_sources(&ctx, &env);

        match stack.active() {
            CredentialSource::Profile { name, source, .. } => {
                assert_eq!(name, "default");
                assert!(matches!(source, ProfileSource::Default));
            }
            _ => panic!("Expected profile credential source"),
        }

        let display = stack.display_name();
        assert!(display.contains("profile 'default'"));
        assert!(display.contains("default"));
    }

    #[test]
    fn test_cli_assume_role_with_stack_args_profile() {
        let env = TestEnv::new(); // Empty env

        let ctx = CredentialDetectionContext {
            cli_profile: None,
            stack_args_profile: Some("base-profile".to_string()),
            cli_assume_role_arn: Some("arn:aws:iam::123:role/DeployRole".to_string()),
            stack_args_assume_role_arn: None,
        };

        let stack = detect_credential_sources(&ctx, &env);

        match stack.active() {
            CredentialSource::AssumeRole {
                base_source,
                role_arn,
                source,
            } => {
                assert_eq!(role_arn, "arn:aws:iam::123:role/DeployRole");
                assert!(matches!(source, AssumeRoleSource::CliFlag));

                match base_source.as_ref() {
                    CredentialSource::Profile { name, source, .. } => {
                        assert_eq!(name, "base-profile");
                        assert!(matches!(source, ProfileSource::StackArgs));
                    }
                    _ => panic!("Expected profile as base source"),
                }
            }
            _ => panic!("Expected AssumeRole credential source"),
        }

        let display = stack.display_name();
        assert!(display.contains("AssumeRole DeployRole"));
        assert!(display.contains("from --assume-role-arn"));
        assert!(display.contains("profile 'base-profile'"));
        assert!(display.contains("from stack-args.yaml"));
    }

    #[test]
    fn test_stack_args_assume_role_with_cli_profile() {
        let env = TestEnv::new(); // Empty env

        let ctx = CredentialDetectionContext {
            cli_profile: Some("my-profile".to_string()),
            stack_args_profile: None,
            cli_assume_role_arn: None,
            stack_args_assume_role_arn: Some("arn:aws:iam::456:role/AppRole".to_string()),
        };

        let stack = detect_credential_sources(&ctx, &env);

        match stack.active() {
            CredentialSource::AssumeRole {
                base_source,
                role_arn,
                source,
            } => {
                assert_eq!(role_arn, "arn:aws:iam::456:role/AppRole");
                assert!(matches!(source, AssumeRoleSource::StackArgs));

                match base_source.as_ref() {
                    CredentialSource::Profile { name, source, .. } => {
                        assert_eq!(name, "my-profile");
                        assert!(matches!(source, ProfileSource::CliFlag));
                    }
                    _ => panic!("Expected profile as base source"),
                }
            }
            _ => panic!("Expected AssumeRole credential source"),
        }

        let display = stack.display_name();
        assert!(display.contains("AssumeRole AppRole"));
        assert!(display.contains("from stack-args.yaml"));
        assert!(display.contains("profile 'my-profile'"));
        assert!(display.contains("from --profile"));
    }

    #[test]
    fn test_container_credentials_ecs() {
        let mut env = TestEnv::new();
        env.set(
            "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI",
            "/v2/credentials/test",
        );

        let ctx = CredentialDetectionContext {
            cli_profile: None,
            stack_args_profile: None,
            cli_assume_role_arn: None,
            stack_args_assume_role_arn: None,
        };

        let stack = detect_credential_sources(&ctx, &env);

        assert!(matches!(
            stack.active(),
            CredentialSource::ContainerCredentialsEcs
        ));

        let display = stack.display_name();
        assert!(display.contains("ECS container credentials"));
    }

    #[test]
    fn test_web_identity_token() {
        let mut env = TestEnv::new();
        env.set("AWS_WEB_IDENTITY_TOKEN_FILE", "/path/to/token");
        env.set("AWS_ROLE_ARN", "arn:aws:iam::123:role/WebRole");

        let ctx = CredentialDetectionContext {
            cli_profile: None,
            stack_args_profile: None,
            cli_assume_role_arn: None,
            stack_args_assume_role_arn: None,
        };

        let stack = detect_credential_sources(&ctx, &env);

        assert!(matches!(stack.active(), CredentialSource::WebIdentityToken));

        let display = stack.display_name();
        assert!(display.contains("web identity token"));
    }

    #[test]
    fn test_credential_source_stack_methods() {
        let sources = vec![
            CredentialSource::EnvironmentVariablesStatic,
            CredentialSource::Profile {
                name: "test".to_string(),
                source: ProfileSource::StackArgs,
                profile_role_arn: None,
            },
        ];

        let stack = CredentialSourceStack::new(sources);

        // Test active()
        assert!(matches!(
            stack.active(),
            CredentialSource::EnvironmentVariablesStatic
        ));

        // Test overridden()
        assert_eq!(stack.overridden().len(), 1);
        match &stack.overridden()[0] {
            CredentialSource::Profile { name, .. } => assert_eq!(name, "test"),
            _ => panic!("Expected profile in overridden"),
        }

        // Test display_name()
        let display = stack.display_name();
        assert!(display.contains("environment variables"));
        assert!(display.contains("overriding"));
        assert!(display.contains("profile 'test'"));
    }
}
