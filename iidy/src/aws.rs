use anyhow::Result;
use aws_config::BehaviorVersion;
use aws_config::SdkConfig;
use aws_config::sts::AssumeRoleProvider;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_types::region::Region;

use crate::cli::NormalizedAwsOpts;

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
pub async fn config_from_merged_settings(merged_settings: &AwsSettings) -> Result<SdkConfig> {
    // Set AWS_SDK_LOAD_CONFIG if ~/.aws exists (matching iidy-js behavior)
    if let Some(home) = std::env::var_os("HOME") {
        let aws_dir = std::path::Path::new(&home).join(".aws");
        if aws_dir.exists() {
            // SAFETY: This is called early in the program before any threads are spawned
            unsafe {
                std::env::set_var("AWS_SDK_LOAD_CONFIG", "1");
            }
        }
    }

    let mut loader = aws_config::defaults(BehaviorVersion::v2025_01_17());

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

    Ok(config)
}

/// Load AWS SDK configuration using values from [`NormalizedAwsOpts`].
///
/// This honors the `region`, `profile`, and `assume_role_arn` fields of
/// `AwsOpts`. The returned [`SdkConfig`] can be used to construct AWS service
/// clients.
pub async fn config_from_normalized_opts(opts: &NormalizedAwsOpts) -> Result<SdkConfig> {
    let settings = AwsSettings::from_normalized_opts(opts);
    config_from_merged_settings(&settings).await
}
