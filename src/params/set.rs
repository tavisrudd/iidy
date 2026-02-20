use anyhow::Result;
use aws_sdk_ssm::types::Tag;

use crate::cli::{Cli, ParamSetArgs};
use crate::params::{
    MESSAGE_TAG, create_kms_client, create_ssm_client, get_kms_alias_for_parameter, set_param_tags,
};

pub async fn set_param(cli: &Cli, args: &ParamSetArgs) -> Result<i32> {
    let opts = cli.aws_opts.clone().normalize();
    let (ssm, config) = create_ssm_client(&opts).await?;

    let name = if args.with_approval {
        format!("{}.pending", args.path)
    } else {
        args.path.clone()
    };

    let param_type: aws_sdk_ssm::types::ParameterType = args.r#type.as_str().into();

    let key_id = if args.r#type == "SecureString" {
        let kms = create_kms_client(&config).await;
        get_kms_alias_for_parameter(&kms, &name).await?
    } else {
        None
    };

    let mut req = ssm
        .put_parameter()
        .name(&name)
        .value(&args.value)
        .r#type(param_type)
        .overwrite(args.overwrite);
    if let Some(key) = &key_id {
        req = req.key_id(key);
    }
    req.send().await?;

    if args.with_approval {
        let region = config
            .region()
            .map(|r| r.to_string())
            .unwrap_or_else(|| "us-east-1".to_string());
        println!("Parameter change is pending approval. Review change with:");
        println!("  iidy --region {} param review {}", region, args.path);
    }

    if let Some(message) = &args.message {
        let tag = Tag::builder().key(MESSAGE_TAG).value(message).build()?;
        set_param_tags(&ssm, &name, vec![tag]).await?;
    }

    Ok(0)
}
