use anyhow::Result;
use aws_config::BehaviorVersion;
use aws_config::SdkConfig;
use aws_config::sts::AssumeRoleProvider;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_types::region::Region;

use crate::cli::NormalizedAwsOpts;

pub mod client_req_token;
mod credential_source;
pub mod timing;

pub use credential_source::{
    AssumeRoleSource, CredentialDetectionContext, CredentialSource, CredentialSourceStack,
    EnvVarProvider, ProfileSource, SystemEnv, detect_credential_sources,
};

/// Custom error type for user-friendly AWS errors that have already been displayed
#[derive(Debug)]
pub struct UserFriendlyAwsError {
    pub message: String,
    pub exit_code: i32,
}

impl std::fmt::Display for UserFriendlyAwsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UserFriendlyAwsError {}

/// Format AWS errors in a user-friendly way
pub fn format_aws_error(error: &anyhow::Error) -> String {
    let error_chain: Vec<String> = error.chain().map(|e| e.to_string()).collect();

    // Look for common AWS error patterns and provide user-friendly messages
    for err_msg in &error_chain {
        let lower_msg = err_msg.to_lowercase();

        if lower_msg.contains("expiredtoken") || lower_msg.contains("expired") {
            return "ERROR: AWS credentials have expired. Please refresh your credentials."
                .to_string();
        }

        if lower_msg.contains("no providers in chain provided credentials") {
            return "ERROR: AWS credentials not found. Please configure your AWS credentials."
                .to_string();
        }

        if lower_msg.contains("access denied") || lower_msg.contains("unauthorized") {
            return "ERROR: Access denied. Please check your AWS permissions.".to_string();
        }

        if lower_msg.contains("invalid security token") {
            return "ERROR: Invalid AWS security token. Please refresh your credentials."
                .to_string();
        }

        if lower_msg.contains("network") || lower_msg.contains("timeout") {
            return "ERROR: Network error connecting to AWS. Please check your internet connection.".to_string();
        }
    }

    // If no specific pattern matches, show the most relevant error from the chain
    if error_chain.len() > 1 {
        format!("ERROR: AWS error - {}", error_chain[error_chain.len() - 1])
    } else {
        format!("ERROR: {error}")
    }
}

/// Display a user-friendly AWS error message and return a custom error that won't print additional details
pub fn display_and_return_user_friendly_error(error: &anyhow::Error) -> UserFriendlyAwsError {
    let message = format_aws_error(error);
    eprintln!("{message}");
    UserFriendlyAwsError {
        message: "User-friendly error already displayed".to_string(),
        exit_code: 1,
    }
}

/// AWS settings structure for merging CLI and stack-args.yaml settings
#[derive(Debug, Clone, Default)]
pub struct AwsSettings {
    pub profile: Option<String>,
    pub region: Option<String>,
    pub assume_role_arn: Option<String>,
}

impl AwsSettings {
    /// Create AwsSettings from CLI options (normalized)
    pub fn from_normalized_opts(opts: &NormalizedAwsOpts) -> Self {
        Self {
            profile: opts.profile.clone(),
            region: opts.region.clone(),
            assume_role_arn: opts.assume_role_arn.clone(),
        }
    }
}

/// Load AWS SDK configuration from merged settings (iidy-js configureAWS equivalent)
pub async fn config_from_merged_settings(
    merged_settings: &AwsSettings,
    detection_ctx: &CredentialDetectionContext,
) -> Result<(SdkConfig, CredentialSourceStack)> {
    // Detect credential sources BEFORE loading config
    let credential_sources = detect_credential_sources(detection_ctx, &SystemEnv);

    let mut loader = aws_config::defaults(BehaviorVersion::v2026_01_12());

    if let Some(ref region) = merged_settings.region {
        loader = loader.region(Region::new(region.clone()));
    }

    if let Some(ref profile) = merged_settings.profile {
        loader = loader.profile_name(profile.clone());
    }

    // Load base configuration from the default chain
    let base_config = loader.load().await;

    // Start building the final config from the base configuration
    let mut builder = base_config.clone().into_builder();

    if let Some(ref role) = merged_settings.assume_role_arn {
        let provider = AssumeRoleProvider::builder(role)
            .configure(&base_config)
            .session_name("iidy")
            .build()
            .await;
        builder = builder.credentials_provider(SharedCredentialsProvider::new(provider));
    }

    // Note: The Rust SDK doesn't expose maxRetries at the config level,
    // but individual service clients can configure retry behavior

    let config = builder.build();

    Ok((config, credential_sources))
}

/// Load AWS SDK configuration using values from [`NormalizedAwsOpts`].
///
/// This honors the `region`, `profile`, and `assume_role_arn` fields of
/// `AwsOpts`. The returned [`SdkConfig`] can be used to construct AWS service
/// clients.
///
/// Note: This is used for commands that don't use stack-args.yaml. All settings
/// are treated as coming from CLI flags for credential source detection.
pub async fn config_from_normalized_opts(
    opts: &NormalizedAwsOpts,
) -> Result<(SdkConfig, CredentialSourceStack)> {
    let settings = AwsSettings::from_normalized_opts(opts);

    // Create detection context - all settings come from CLI in this path
    let detection_ctx = CredentialDetectionContext {
        cli_profile: opts.profile.clone(),
        stack_args_profile: None,
        cli_assume_role_arn: opts.assume_role_arn.clone(),
        stack_args_assume_role_arn: None,
    };

    config_from_merged_settings(&settings, &detection_ctx).await
}
