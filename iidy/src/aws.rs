use anyhow::Result;
use aws_config::BehaviorVersion;
use aws_config::SdkConfig;
use aws_config::sts::AssumeRoleProvider;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_types::region::Region;

use crate::cli::{AwsOpts, NormalizedAwsOpts};

/// Load AWS SDK configuration using values from [`AwsOpts`].
///
/// This honors the `region`, `profile`, and `assume_role_arn` fields of
/// `AwsOpts`. The returned [`SdkConfig`] can be used to construct AWS service
/// clients.
pub async fn config_from_opts(opts: &AwsOpts) -> Result<SdkConfig> {
    let mut loader = aws_config::defaults(BehaviorVersion::v2025_01_17());

    if let Some(ref region) = opts.region {
        loader = loader.region(Region::new(region.clone()));
    }

    if let Some(ref profile) = opts.profile {
        loader = loader.profile_name(profile.clone());
    }

    // Load base configuration from the default chain
    let base_config = loader.load().await;

    // Start building the final config from the base configuration
    let mut builder = base_config.clone().into_builder();

    if let Some(ref role) = opts.assume_role_arn {
        let provider = AssumeRoleProvider::builder(role)
            .configure(&base_config)
            .session_name("iidy")
            .build()
            .await;
        builder = builder.credentials_provider(SharedCredentialsProvider::new(provider));
    }

    let config = builder.build();

    Ok(config)
}

/// Load AWS SDK configuration using values from [`NormalizedAwsOpts`].
/// 
/// This is a convenience function that extracts the relevant AWS configuration
/// fields from NormalizedAwsOpts and delegates to config_from_opts.
pub async fn config_from_normalized_opts(opts: &NormalizedAwsOpts) -> Result<SdkConfig> {
    // Convert NormalizedAwsOpts back to AwsOpts for the configuration
    let aws_opts = AwsOpts {
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        assume_role_arn: opts.assume_role_arn.clone(),
        client_request_token: None, // Token is handled separately in NormalizedAwsOpts
    };
    
    config_from_opts(&aws_opts).await
}
