use crate::cli::{GetTemplateArgs, TemplateFormat, TemplateStageArg, Cli};
use crate::cfn::create_context_for_operation;
use anyhow::Result;
use aws_sdk_cloudformation::operation::get_template::GetTemplateOutput;
use aws_sdk_cloudformation::types::TemplateStage;
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

/// Output of formatting a stack template.
pub struct FormattedTemplate {
    /// Lines that should be printed to stderr.
    pub stderr_lines: Vec<String>,
    /// The template content to print to stdout.
    pub body: String,
}

enum TemplateBody {
    Json(JsonValue),
    Yaml(YamlValue),
}

impl TemplateBody {
    fn to_json(&self) -> Result<JsonValue> {
        match self {
            TemplateBody::Json(j) => Ok(j.clone()),
            TemplateBody::Yaml(y) => Ok(serde_json::to_value(y)?),
        }
    }

    fn to_yaml(&self) -> Result<YamlValue> {
        match self {
            TemplateBody::Yaml(y) => Ok(y.clone()),
            TemplateBody::Json(j) => Ok(serde_yaml::to_value(j)?),
        }
    }
}

fn parse_template_body(s: &str) -> Result<TemplateBody> {
    let s = s.trim_start();
    if s.starts_with('{') {
        Ok(TemplateBody::Json(serde_json::from_str(s)?))
    } else {
        Ok(TemplateBody::Yaml(serde_yaml::from_str(s)?))
    }
}

fn strip_trailing_newline(mut s: String) -> String {
    if s.ends_with('\n') {
        s.pop();
    }
    s
}

/// Format the template returned from AWS according to the requested stage
/// and output format.
pub fn format_template(
    output: GetTemplateOutput,
    stage: TemplateStageArg,
    format: TemplateFormat,
) -> Result<FormattedTemplate> {
    let stages = output
        .stages_available()
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let stderr_lines = vec![
        format!("# Stages Available: {stages}"),
        format!("# Stage Shown: {stage}"),
        String::new(),
    ];
    let body_raw = output.template_body().unwrap_or_default();
    let template = parse_template_body(body_raw)?;
    let body = match format {
        TemplateFormat::Yaml => serde_yaml::to_string(&template.to_yaml()?)?,
        TemplateFormat::Json => serde_json::to_string(&template.to_json()?)?,
        TemplateFormat::Original => body_raw.to_string(),
    };
    Ok(FormattedTemplate {
        stderr_lines,
        body: strip_trailing_newline(body),
    })
}

/// Retrieve a stack template from CloudFormation and format it for display.
///
/// This is a read-only operation
/// - stderr: "# Stages Available: ..." and "# Stage Shown: ..."
/// - stdout: Template content in requested format
/// - No progress messages or command metadata
pub async fn get_stack_template(cli: &Cli, args: &GetTemplateArgs) -> Result<FormattedTemplate> {
    let operation = cli.command.to_cfn_operation();
    let opts = cli.aws_opts.clone().normalize();
    let context = create_context_for_operation(&opts, operation).await?;
    let client = &context.client;
    let stage = match args.stage {
        TemplateStageArg::Original => TemplateStage::Original,
        TemplateStageArg::Processed => TemplateStage::Processed,
    };

    let output = client
        .get_template()
        .stack_name(&args.stackname)
        .template_stage(stage)
        .send()
        .await?;

    format_template(output, args.stage.clone(), args.format.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_cloudformation::types::TemplateStage;

    fn sample_output(body: &str) -> GetTemplateOutput {
        GetTemplateOutput::builder()
            .template_body(body)
            .stages_available(TemplateStage::Original)
            .stages_available(TemplateStage::Processed)
            .build()
    }

    #[test]
    fn formats_yaml() {
        let output = sample_output("{\"A\":1}");
        let formatted =
            format_template(output, TemplateStageArg::Original, TemplateFormat::Yaml).unwrap();
        assert_eq!(formatted.stderr_lines.len(), 3);
        assert!(formatted.stderr_lines[0].contains("Stages Available"));
        assert!(formatted.body.contains("A: 1"));
    }

    #[test]
    fn parses_json_and_yaml() {
        match parse_template_body("{\"A\":1}").unwrap() {
            TemplateBody::Json(v) => assert_eq!(v["A"], 1),
            _ => panic!("expected json"),
        }
        match parse_template_body("A: 1").unwrap() {
            TemplateBody::Yaml(v) => assert_eq!(v["A"], 1),
            _ => panic!("expected yaml"),
        }
    }

    #[test]
    fn strips_trailing_newline() {
        assert_eq!(strip_trailing_newline("abc\n".to_string()), "abc");
        assert_eq!(strip_trailing_newline("abc".to_string()), "abc");
    }
}
