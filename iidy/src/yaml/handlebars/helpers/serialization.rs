//! Serialization helpers for handlebars templates
//!
//! Provides helpers to convert data to JSON and YAML formats

use handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext};

/// toJson helper - converts value to JSON string
pub fn to_json_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0).ok_or_else(|| {
        handlebars::RenderError::new("toJson helper requires exactly one parameter")
    })?;

    let json_str = serde_json::to_string(param.value()).map_err(|e| {
        handlebars::RenderError::new(&format!("Failed to serialize to JSON: {}", e))
    })?;

    out.write(&json_str)?;
    Ok(())
}

/// toJsonPretty helper - converts value to pretty-printed JSON string
pub fn to_json_pretty_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0).ok_or_else(|| {
        handlebars::RenderError::new("toJsonPretty helper requires exactly one parameter")
    })?;

    let json_str = serde_json::to_string_pretty(param.value()).map_err(|e| {
        handlebars::RenderError::new(&format!("Failed to serialize to pretty JSON: {}", e))
    })?;

    out.write(&json_str)?;
    Ok(())
}

/// toYaml helper - converts value to YAML string
pub fn to_yaml_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0).ok_or_else(|| {
        handlebars::RenderError::new("toYaml helper requires exactly one parameter")
    })?;

    let yaml_str = serde_yaml::to_string(param.value()).map_err(|e| {
        handlebars::RenderError::new(&format!("Failed to serialize to YAML: {}", e))
    })?;

    out.write(&yaml_str)?;
    Ok(())
}
