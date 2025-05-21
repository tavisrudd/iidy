use anyhow::Result;
use aws_sdk_cloudformation::{types::TemplateStage, Client};
use aws_sdk_cloudformation::operation::get_template::GetTemplateOutput;
use serde_yaml::Value as YamlValue;
use crate::{aws, cli::{AwsOpts, GetTemplateArgs}};

fn json_escape(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

fn yaml_value_to_json(value: &YamlValue) -> String {
    fn helper(v: &YamlValue, indent: usize, out: &mut String) {
        match v {
            YamlValue::Null => out.push_str("null"),
            YamlValue::Bool(b) => out.push_str(&b.to_string()),
            YamlValue::Number(n) => out.push_str(&n.to_string()),
            YamlValue::String(s) => {
                out.push('"');
                out.push_str(&json_escape(s));
                out.push('"');
            }
            YamlValue::Sequence(seq) => {
                if seq.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push('[');
                out.push('\n');
                let next_indent = indent + 2;
                for (i, item) in seq.iter().enumerate() {
                    out.push_str(&" ".repeat(next_indent));
                    helper(item, next_indent, out);
                    if i + 1 != seq.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&" ".repeat(indent));
                out.push(']');
            }
            YamlValue::Mapping(map) => {
                if map.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push('{');
                out.push('\n');
                let next_indent = indent + 2;
                let len = map.len();
                for (idx, (k, val)) in map.iter().enumerate() {
                    out.push_str(&" ".repeat(next_indent));
                    let key = match k {
                        YamlValue::String(s) => json_escape(s),
                        other => {
                            let mut tmp = String::new();
                            helper(other, 0, &mut tmp);
                            tmp
                        }
                    };
                    out.push('"');
                    out.push_str(&key);
                    out.push_str("\": ");
                    helper(val, next_indent, out);
                    if idx + 1 != len {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&" ".repeat(indent));
                out.push('}');
            }
            YamlValue::Tagged(t) => helper(&t.value, indent, out),
        }
    }

    let mut out = String::new();
    helper(value, 0, &mut out);
    out
}

/// Output of formatting a stack template.
pub struct FormattedTemplate {
    /// Lines that should be printed to stderr.
    pub stderr_lines: Vec<String>,
    /// The template content to print to stdout.
    pub body: String,
}

/// Format the template returned from AWS according to the requested stage
/// and output format.
pub fn format_template(
    output: GetTemplateOutput,
    stage: &str,
    format: &str,
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
    let body = match format {
        "yaml" => {
            let value: YamlValue = serde_yaml::from_str(body_raw)?;
            // serde_yaml emits trailing newline; trim to keep output similar to list_stacks
            let mut text = serde_yaml::to_string(&value)?;
            if text.ends_with('\n') { text.pop(); }
            text
        }
        "json" => {
            let value: YamlValue = serde_yaml::from_str(body_raw)?;
            yaml_value_to_json(&value)
        }
        _ => body_raw.to_string(),
    };

    Ok(FormattedTemplate { stderr_lines, body })
}

/// Retrieve a stack template from CloudFormation and format it for display.
pub async fn get_stack_template(
    opts: &AwsOpts,
    args: &GetTemplateArgs,
) -> Result<FormattedTemplate> {
    let config = aws::config_from_opts(opts).await?;
    let client = Client::new(&config);

    let stage = TemplateStage::from(args.stage.as_str());

    let output = client
        .get_template()
        .stack_name(&args.stackname)
        .template_stage(stage)
        .send()
        .await?;

    format_template(output, &args.stage, &args.format)
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
        let formatted = format_template(output, "Original", "yaml").unwrap();
        assert_eq!(formatted.stderr_lines.len(), 3);
        assert!(formatted.stderr_lines[0].contains("Stages Available"));
        assert!(formatted.body.contains("A: 1"));
    }
}
