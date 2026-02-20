use anyhow::Result;
use std::collections::BTreeMap;

use crate::cli::{Cli, ParamGetByPathArgs};
use crate::params::{ParamOutput, create_ssm_client, format_output, get_param_tags};

pub async fn get_by_path(cli: &Cli, args: &ParamGetByPathArgs) -> Result<i32> {
    let opts = cli.aws_opts.clone().normalize();
    let (ssm, _config) = create_ssm_client(&opts).await?;

    let mut parameters: Vec<aws_sdk_ssm::types::Parameter> = Vec::new();
    let mut next_token: Option<String> = None;

    loop {
        let mut req = ssm
            .get_parameters_by_path()
            .path(&args.path)
            .recursive(args.recursive)
            .with_decryption(args.decrypt);
        if let Some(token) = &next_token {
            req = req.next_token(token);
        }
        let resp = req.send().await?;
        parameters.extend(resp.parameters().to_vec());
        next_token = resp.next_token().map(|s| s.to_string());
        if next_token.is_none() {
            break;
        }
    }

    if parameters.is_empty() {
        println!("No parameters found");
        return Ok(1);
    }

    parameters.sort_by(|a, b| a.name().cmp(&b.name()));

    if args.format == "simple" {
        let sorted_map: BTreeMap<String, String> = parameters
            .iter()
            .map(|p| {
                (
                    p.name().unwrap_or("").to_string(),
                    p.value().unwrap_or("").to_string(),
                )
            })
            .collect();
        println!("{}", serde_yaml::to_string(&sorted_map)?);
    } else {
        let mut tagged_map: BTreeMap<String, ParamOutput> = BTreeMap::new();
        for param in &parameters {
            let name = param.name().unwrap_or("").to_string();
            let tags = get_param_tags(&ssm, &name).await?;
            tagged_map.insert(name, ParamOutput::from_parameter(param).with_tags(tags));
        }
        println!("{}", format_output(&args.format, &tagged_map)?);
    }

    Ok(0)
}
