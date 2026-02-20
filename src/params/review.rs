use anyhow::Result;
use aws_sdk_ssm::types::Tag;

use crate::cli::{Cli, ParamPathArg};
use crate::output::manager::{DynamicOutputManager, OutputOptions};
use crate::params::{
    MESSAGE_TAG, create_kms_client, create_ssm_client, get_kms_alias_for_parameter, get_param_tags,
    maybe_fetch_param, set_param_tags,
};

pub async fn review_param(cli: &Cli, args: &ParamPathArg) -> Result<i32> {
    let opts = cli.aws_opts.clone().normalize();
    let (ssm, config) = create_ssm_client(&opts).await?;

    let name = &args.path;
    let pending_name = format!("{}.pending", name);

    let pending_param = maybe_fetch_param(&ssm, &pending_name, true).await?;

    let pending_param = match pending_param {
        Some(p) => p,
        None => {
            println!("There is no pending change for parameter {}", name);
            return Ok(1);
        }
    };

    let current_param = maybe_fetch_param(&ssm, name, true).await?;
    let pending_tags = get_param_tags(&ssm, &pending_name).await?;

    let value = pending_param.value().unwrap_or("").to_string();
    let current_value = current_param
        .as_ref()
        .and_then(|p| p.value())
        .unwrap_or("<not set>");
    let param_type = pending_param
        .r#type()
        .map(|t| t.as_str().to_string())
        .unwrap_or_else(|| "SecureString".to_string());

    println!("Current: {}", current_value);
    println!("Pending: {}", value);
    println!();

    if let Some(message) = pending_tags.get(MESSAGE_TAG) {
        println!("Message: {}", message);
        println!();
    }

    let output_options = OutputOptions::new(cli.clone());
    let mut output_manager =
        DynamicOutputManager::new(cli.global_opts.effective_output_mode(), output_options).await?;

    let confirmed = output_manager
        .request_confirmation("Would you like to approve these changes?".to_string())
        .await?;

    if confirmed {
        let key_id = if param_type == "SecureString" {
            let kms = create_kms_client(&config).await;
            get_kms_alias_for_parameter(&kms, name).await?
        } else {
            None
        };

        let param_type_enum: aws_sdk_ssm::types::ParameterType = param_type.as_str().into();
        let mut req = ssm
            .put_parameter()
            .name(name)
            .value(&value)
            .r#type(param_type_enum)
            .overwrite(true);
        if let Some(key) = &key_id {
            req = req.key_id(key);
        }
        req.send().await?;

        ssm.delete_parameter().name(&pending_name).send().await?;

        let tags: Vec<Tag> = pending_tags
            .iter()
            .map(|(k, v)| Tag::builder().key(k).value(v).build())
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if !tags.is_empty() {
            set_param_tags(&ssm, name, tags).await?;
        }

        Ok(0)
    } else {
        Ok(130)
    }
}
