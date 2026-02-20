use anyhow::Result;

use crate::cfn::CfnContext;
use crate::output::data::TemplateValidation;

/// Validate template using CloudFormation ValidateTemplate API
pub async fn validate_template(
    context: &CfnContext,
    template_body: &str,
) -> Result<TemplateValidation> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if template_body.len() > 51200 {
        warnings.push(
            "Template exceeds 51200 bytes; skipping CFN validation (will be validated on deploy)"
                .to_string(),
        );
    } else {
        let validation_request = context
            .client
            .validate_template()
            .template_body(template_body);
        match validation_request.send().await {
            Ok(_) => {}
            Err(e) => errors.push(format!("Template validation failed: {e}")),
        }
    }

    Ok(TemplateValidation {
        enabled: true,
        errors,
        warnings,
    })
}
