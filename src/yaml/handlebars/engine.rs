//! Handlebars engine setup and template interpolation
//!
//! Core functionality for creating and configuring the handlebars registry
//! and processing template strings.

use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Result, anyhow};
use handlebars::Handlebars;
use serde_json::Value;

use super::helpers::*;

static REGISTRY: OnceLock<Handlebars<'static>> = OnceLock::new();

fn get_registry() -> &'static Handlebars<'static> {
    REGISTRY.get_or_init(|| {
        let mut handlebars = Handlebars::new();

        handlebars.set_strict_mode(true);
        handlebars.register_escape_fn(handlebars::no_escape);

        // JSON helpers
        handlebars.register_helper("toJson", Box::new(to_json_helper));
        handlebars.register_helper("tojson", Box::new(to_json_helper));
        handlebars.register_helper("toJsonPretty", Box::new(to_json_pretty_helper));
        handlebars.register_helper("tojsonPretty", Box::new(to_json_pretty_helper));

        // YAML helper
        handlebars.register_helper("toYaml", Box::new(to_yaml_helper));
        handlebars.register_helper("toyaml", Box::new(to_yaml_helper));

        // Encoding helpers
        handlebars.register_helper("base64", Box::new(base64_helper));
        handlebars.register_helper("urlEncode", Box::new(url_encode_helper));
        handlebars.register_helper("sha256", Box::new(sha256_helper));

        // String manipulation helpers
        handlebars.register_helper("toLowerCase", Box::new(to_lower_case_helper));
        handlebars.register_helper("toUpperCase", Box::new(to_upper_case_helper));
        handlebars.register_helper("titleize", Box::new(titleize_helper));
        handlebars.register_helper("camelCase", Box::new(camel_case_helper));
        handlebars.register_helper("pascalCase", Box::new(pascal_case_helper));
        handlebars.register_helper("snakeCase", Box::new(snake_case_helper));
        handlebars.register_helper("kebabCase", Box::new(kebab_case_helper));
        handlebars.register_helper("capitalize", Box::new(capitalize_helper));
        handlebars.register_helper("trim", Box::new(trim_helper));
        handlebars.register_helper("replace", Box::new(replace_helper));
        handlebars.register_helper("substring", Box::new(substring_helper));
        handlebars.register_helper("length", Box::new(length_helper));
        handlebars.register_helper("pad", Box::new(pad_helper));
        handlebars.register_helper("concat", Box::new(concat_helper));

        // Object access helpers
        handlebars.register_helper("lookup", Box::new(lookup_helper));

        handlebars
    })
}

/// Interpolate a handlebars template string with the given environment values
pub fn interpolate_handlebars_string(
    template_string: &str,
    env_values: &HashMap<String, Value>,
    error_context: &str,
) -> Result<String> {
    if !template_string.contains("{{") {
        return Ok(template_string.to_string());
    }

    let handlebars = get_registry();

    let data = Value::Object(
        env_values
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    );

    handlebars
        .render_template(template_string, &data)
        .map_err(|e| {
            anyhow!(
                "Error in string template at {}: {}\nTemplate: {}",
                error_context,
                e,
                template_string
            )
        })
}
