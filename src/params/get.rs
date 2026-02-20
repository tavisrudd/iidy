use anyhow::Result;

use crate::cli::{Cli, ParamGetArgs};
use crate::params::{ParamOutput, create_ssm_client, format_output, get_param_tags};

pub async fn get_param(cli: &Cli, args: &ParamGetArgs) -> Result<i32> {
    let opts = cli.aws_opts.clone().normalize();
    let (ssm, _config) = create_ssm_client(&opts).await?;

    let resp = ssm
        .get_parameter()
        .name(&args.path)
        .with_decryption(args.decrypt)
        .send()
        .await?;

    let param = resp
        .parameter()
        .ok_or_else(|| anyhow::anyhow!("Parameter lookup error"))?;

    if args.format == "simple" {
        println!("{}", param.value().unwrap_or(""));
    } else {
        let tags = get_param_tags(&ssm, &args.path).await?;
        let output = ParamOutput::from_parameter(param).with_tags(tags);
        println!("{}", format_output(&args.format, &output)?);
    }

    Ok(0)
}
