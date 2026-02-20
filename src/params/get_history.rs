use anyhow::Result;
use serde::Serialize;

use crate::cli::{Cli, ParamGetArgs};
use crate::params::{
    MESSAGE_TAG, ParamHistoryOutput, create_ssm_client, format_output, get_param_tags,
};

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SimpleHistoryCurrent {
    value: Option<String>,
    #[serde(rename = "LastModifiedDate")]
    last_modified_date: Option<String>,
    #[serde(rename = "LastModifiedUser")]
    last_modified_user: Option<String>,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SimpleHistoryPrevious {
    value: Option<String>,
    #[serde(rename = "LastModifiedDate")]
    last_modified_date: Option<String>,
    #[serde(rename = "LastModifiedUser")]
    last_modified_user: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SimpleHistory {
    current: SimpleHistoryCurrent,
    previous: Vec<SimpleHistoryPrevious>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct FullHistory {
    current: ParamHistoryOutput,
    previous: Vec<ParamHistoryOutput>,
}

pub async fn get_history(cli: &Cli, args: &ParamGetArgs) -> Result<i32> {
    let opts = cli.aws_opts.clone().normalize();
    let (ssm, _config) = create_ssm_client(&opts).await?;

    let mut history: Vec<aws_sdk_ssm::types::ParameterHistory> = Vec::new();
    let mut next_token: Option<String> = None;

    loop {
        let mut req = ssm
            .get_parameter_history()
            .name(&args.path)
            .with_decryption(args.decrypt);
        if let Some(token) = &next_token {
            req = req.next_token(token);
        }
        let resp = req.send().await?;
        history.extend(resp.parameters().to_vec());
        next_token = resp.next_token().map(|s| s.to_string());
        if next_token.is_none() {
            break;
        }
    }

    history.sort_by(|a, b| a.last_modified_date().cmp(&b.last_modified_date()));

    let current = history
        .last()
        .ok_or_else(|| anyhow::anyhow!("No history found for parameter '{}'", args.path))?;

    let previous = &history[..history.len().saturating_sub(1)];

    let tags = get_param_tags(&ssm, &args.path).await?;

    if args.format == "simple" {
        let message = tags.get(MESSAGE_TAG).cloned().unwrap_or_default();

        let output = SimpleHistory {
            current: SimpleHistoryCurrent {
                value: current.value().map(|s| s.to_string()),
                last_modified_date: current
                    .last_modified_date()
                    .map(crate::params::format_aws_datetime),
                last_modified_user: current.last_modified_user().map(|s| s.to_string()),
                message,
            },
            previous: previous
                .iter()
                .map(|p| SimpleHistoryPrevious {
                    value: p.value().map(|s| s.to_string()),
                    last_modified_date: p
                        .last_modified_date()
                        .map(crate::params::format_aws_datetime),
                    last_modified_user: p.last_modified_user().map(|s| s.to_string()),
                })
                .collect(),
        };
        println!("{}", serde_yaml::to_string(&output)?);
    } else {
        let output = FullHistory {
            current: ParamHistoryOutput::from_history(current).with_tags(tags),
            previous: previous
                .iter()
                .map(ParamHistoryOutput::from_history)
                .collect(),
        };
        println!("{}", format_output(&args.format, &output)?);
    }

    Ok(0)
}
